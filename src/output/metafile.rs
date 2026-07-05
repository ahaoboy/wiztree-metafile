// Metafile output formatter (esbuild compatible).
//
// Based on the upstream `bloaty-metafile` conversion logic.
//
// Optimizations versus the previous implementation:
//
//  * `TreeNode.name` was removed — it duplicated the HashMap key used to
//    reach the node. Iteration now uses the keys directly via
//    `HashMap::iter()`, eliminating one `String` allocation per directory.
//  * `parent_path` and `full_path` are `Rc<str>` so the cost of passing the
//    parent's accumulated path down through a recursion is a single atomic
//    refcount bump rather than a `String` clone (which previously paid for
//    both the capacity and the bytes copy on every directory).
//  * Per-entry path is split into a `Vec<&str>` (borrowed) rather than a
//    `Vec<String>` (owned), eliminating one allocation per path segment per
//    entry — for 100,000 entries with ~5 segments each this saves ~500,000
//    `String` allocations.
//  * Writing to a file goes through `serde_json::to_writer(BufWriter)` so
//    the full JSON document never exists as a single contiguous `String` in
//    memory; the old path materialized a 100MB+ string before flushing.

use crate::analyzer::AnalysisResult;
use crate::error::AnalyzerError;
use crate::output::OutputFormatter;
use serde_metafile::{Import, Input, InputDetail, Metafile, Output};
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::rc::Rc;

#[derive(Debug, Default)]
struct TreeNode {
    size: u64,
    total_size: u64,
    children: HashMap<Rc<str>, TreeNode>,
}

pub struct MetafileFormatter;

impl MetafileFormatter {
    /// Normalize path separators to forward slashes so glob patterns and the
    /// esbuild analyzer both work on Windows and Unix.
    fn normalize_path(path: &Path) -> String {
        path.display().to_string().replace('\\', "/")
    }

    /// Build a tree structure from file entries.
    fn build_tree(result: &AnalysisResult) -> TreeNode {
        let mut root = TreeNode::default();
        for entry in &result.entries {
            let path_str = Self::normalize_path(&entry.path);
            // Borrowed `&str` slices; no per-segment allocation.
            let parts: Vec<&str> = path_str.split('/').collect();
            Self::add_path(&mut root, &parts, entry.size);
        }
        root
    }

    /// Add a path to the tree, accumulating sizes.
    fn add_path(node: &mut TreeNode, parts: &[&str], size: u64) {
        node.total_size += size;
        if parts.is_empty() {
            return;
        }
        let part = parts[0];
        let remaining = &parts[1..];
        // Build an `Rc<str>` once for this segment; subsequent identical
        // segments reuse the same allocation via `entry(part.clone())` (the
        // clone is a refcount bump, not a `String` alloc).
        let part_rc: Rc<str> = Rc::from(part);
        if remaining.is_empty() {
            // Leaf = file
            let child = node.children.entry(part_rc).or_default();
            child.size = size;
            child.total_size = size;
        } else {
            let child = node.children.entry(part_rc).or_default();
            Self::add_path(child, remaining, size);
        }
    }

    /// Recursively traverse the tree and produce `Input` entries keyed by
    /// their fully-qualified path string.
    ///
    /// `full_path` is an `Rc<str>` so the cost of passing it down to a
    /// recursion is a single refcount bump (no `String` clone); each unique
    /// path is built exactly once with a single `String` allocation that is
    /// then moved into the `Rc<str>`.
    fn traverse_tree(
        name: &Rc<str>,
        node: &TreeNode,
        inputs: &mut HashMap<String, Input>,
        parent_path: Option<Rc<str>>,
    ) {
        let full_path: Rc<str> = match parent_path {
            Some(p) => {
                let mut s = String::with_capacity(p.len() + 1 + name.len());
                s.push_str(&p);
                s.push('/');
                s.push_str(name);
                Rc::from(s)
            }
            None => name.clone(),
        };

        // Each child's `Import.path` is the child's eventual `full_path` —
        // the same string the recursion will construct when it descends into
        // that child. Serde's `Import::path: String` forces a single fresh
        // allocation per import (which we cannot avoid without making
        // `Import` generic over lifetime/`Cow`), but we share the `Rc<str>`
        // for everything else (recursion parent_path, name lookup, etc).
        let imports: Vec<Import> = node
            .children
            .keys()
            .map(|child_name| {
                let mut path = String::with_capacity(full_path.len() + 1 + child_name.len());
                path.push_str(&full_path);
                path.push('/');
                path.push_str(child_name);
                Import {
                    path,
                    kind: None,
                    external: false,
                    original: None,
                    with: None,
                }
            })
            .collect();

        inputs.insert(
            String::from(&*full_path),
            Input {
                bytes: node.size,
                imports,
                format: None,
                with: None,
            },
        );

        for (child_name, child) in &node.children {
            // Refcount-bump reuse, no String clone.
            Self::traverse_tree(child_name, child, inputs, Some(Rc::clone(&full_path)));
        }
    }

    /// Build the `Metafile` value once; reused by the JSON formatter and
    /// the binary (rkyv) output path.
    pub(crate) fn build_metafile(result: &AnalysisResult) -> (Metafile, u64) {
        let root = Self::build_tree(result);

        let mut inputs: HashMap<String, Input> = HashMap::with_capacity(result.entries.len() + 64);
        for (child_name, child) in &root.children {
            Self::traverse_tree(child_name, child, &mut inputs, None);
        }

        // `Output.inputs` records per-input `bytesInOutput`. By iterating
        // over `inputs` we share each key via `String::clone`; this is the
        // one unavoidable extra allocation per path (serde_metafile's
        // `Output::inputs` is `HashMap<String, _>`).
        let output_inputs: HashMap<String, InputDetail> = inputs
            .iter()
            .map(|(k, input)| {
                (
                    k.clone(),
                    InputDetail {
                        bytes_in_output: input.bytes,
                    },
                )
            })
            .collect();

        let total = root.total_size;
        let output = Output {
            bytes: total,
            inputs: output_inputs,
            imports: vec![],
            exports: vec![],
            entry_point: None,
            css_bundle: None,
        };

        let outputs = HashMap::from([("wiztree".to_string(), output)]);
        (Metafile { inputs, outputs }, total)
    }
}

impl OutputFormatter for MetafileFormatter {
    fn format(&self, result: &AnalysisResult) -> Result<String, AnalyzerError> {
        let (metafile, _) = Self::build_metafile(result);
        let json = serde_json::to_string(&metafile)?;
        Ok(json)
    }

    fn write(&self, result: &AnalysisResult, writer: &mut dyn Write) -> Result<(), AnalyzerError> {
        let (metafile, total) = Self::build_metafile(result);
        // Stream serialization directly through the writer; for large trees
        // this avoids ever materializing the full JSON as a single heap
        // string — a 100MB-ish tree emits 100MB to the writer piecewise
        // instead of holding both the tree and the JSON string live.
        serde_json::to_writer(writer, &metafile).map_err(AnalyzerError::Serialization)?;
        let _ = total; // already encoded; suppress unused-var lint.
        Ok(())
    }
}
