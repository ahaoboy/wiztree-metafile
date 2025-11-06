// Metafile output formatter (esbuild compatible)
// Based on bloaty-metafile conversion logic

use crate::analyzer::AnalysisResult;
use crate::error::AnalyzerError;
use crate::output::OutputFormatter;
use serde_metafile::{Import, Input, InputDetail, Metafile, Output};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Default)]
struct TreeNode {
    name: String,
    size: u64,
    total_size: u64,
    children: HashMap<String, TreeNode>,
}

pub struct MetafileFormatter;

impl MetafileFormatter {
    /// Normalize path separators to forward slashes for consistency
    fn normalize_path(path: &Path) -> String {
        path.display().to_string().replace('\\', "/")
    }

    /// Build a tree structure from file entries
    fn build_tree(result: &AnalysisResult) -> TreeNode {
        let mut root = TreeNode {
            name: "ROOT".to_string(),
            size: 0,
            total_size: 0,
            children: HashMap::new(),
        };

        for entry in &result.entries {
            let path_str = Self::normalize_path(&entry.path);
            let parts: Vec<String> = path_str.split('/').map(String::from).collect();
            Self::add_path(&mut root, &parts, entry.size);
        }

        root
    }

    /// Add a path to the tree, accumulating sizes
    fn add_path(node: &mut TreeNode, parts: &[String], size: u64) {
        node.total_size += size;

        if parts.is_empty() {
            return;
        }

        let part = &parts[0];
        let remaining = &parts[1..];

        if remaining.is_empty() {
            // This is a file (leaf node)
            let child = node.children.entry(part.clone()).or_insert(TreeNode {
                name: part.clone(),
                size,
                total_size: size,
                children: HashMap::new(),
            });
            child.size = size;
            child.total_size = size;
        } else {
            // This is a directory (intermediate node)
            let child = node.children.entry(part.clone()).or_insert(TreeNode {
                name: part.clone(),
                size: 0,
                total_size: 0,
                children: HashMap::new(),
            });
            Self::add_path(child, remaining, size);
        }
    }

    /// Traverse the tree and generate metafile inputs
    fn traverse_tree(
        node: &TreeNode,
        inputs: &mut HashMap<String, Input>,
        parent_path: Option<String>,
    ) {
        let full_path = match &parent_path {
            Some(p) => format!("{}/{}", p, node.name),
            None => node.name.clone(),
        };

        // Generate imports for all children
        let imports: Vec<Import> = node
            .children
            .values()
            .map(|child| Import {
                path: format!("{}/{}", full_path, child.name),
                kind: None,
                external: false,
                original: None,
                with: None,
            })
            .collect();

        // Create input entry for this node
        let input = Input {
            bytes: node.size,
            imports,
            format: None,
            with: None,
        };

        inputs.insert(full_path.clone(), input);

        // Recursively traverse children
        for child in node.children.values() {
            Self::traverse_tree(child, inputs, Some(full_path.clone()));
        }
    }
}

impl OutputFormatter for MetafileFormatter {
    fn format(&self, result: &AnalysisResult) -> Result<String, AnalyzerError> {
        // Build tree structure
        let root = Self::build_tree(result);

        // Generate inputs by traversing the tree
        let mut inputs = HashMap::new();
        for child in root.children.values() {
            Self::traverse_tree(child, &mut inputs, None);
        }

        // Create output entry with all inputs
        let output_inputs: HashMap<String, InputDetail> = inputs
            .iter()
            .map(|(path, input)| {
                (
                    path.clone(),
                    InputDetail {
                        bytes_in_output: input.bytes,
                    },
                )
            })
            .collect();

        let output = Output {
            bytes: root.total_size,
            inputs: output_inputs,
            imports: vec![],
            exports: vec![],
            entry_point: None,
            css_bundle: None,
        };

        let outputs = HashMap::from([("wiztree".to_string(), output)]);
        let metafile = Metafile { inputs, outputs };

        let json = serde_json::to_string_pretty(&metafile)?;
        Ok(json)
    }
}
