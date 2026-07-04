// Depth-first traversal strategy (iterative).
//
// The previous implementation used unbounded recursion, which could blow the
// default 8MB Rust thread stack on deep directory trees (think heavily
// nested `node_modules`). This version uses an explicit `Vec<Vec<DirEntry>>`
// stack where each layer is one `read_dir` batch; the inner stack frame
// therefore yields increasing depth without growing the OS stack at all.

use crate::analyzer::LocalResult;
use crate::config::AnalyzerConfig;
use crate::error::AnalyzerError;
use crate::link_handler::LinkHandler;
use crate::traversal::helpers::{self, TraverseCtx};
use crate::walker::DirEntry;
use std::sync::Arc;

pub fn traverse(
    config: &AnalyzerConfig,
    link_handler: &Arc<LinkHandler>,
) -> Result<LocalResult, AnalyzerError> {
    let ctx = TraverseCtx::new(config, link_handler);
    let mut result = LocalResult::default();
    helpers::bootstrap_root(&ctx, &config.root_path, &mut result)?;

    // Read the root's children first so they all flow through the same
    // `process_entry` pipeline. Avoids a special case for the root below.
    let root_entries = helpers::read_dir_or_warn(&ctx, &config.root_path, 1, &mut result);

    // Explicit stack (Vec<Vec<DirEntry>>): each stack frame is one
    // `read_dir` batch. The inner `Vec::pop` is LIFO which gives us DFS
    // ordering. Using `Vec` (not `VecDeque`) keeps iteration + pop cheap
    // and avoids the worst case of 1k-deep recursion that could overflow
    // the OS stack on heavily nested trees.
    let mut stack: Vec<Vec<DirEntry>> = Vec::with_capacity(16);
    stack.push(root_entries);

    while !stack.is_empty() {
        // Take the next sibling of the current directory without disturbing
        // the rest of the stack. `Vec::pop` on the top batch is O(1).
        let next = {
            let top = stack.last_mut().expect("checked non-empty above");
            top.pop()
        };
        let Some(entry) = next else {
            stack.pop();
            continue;
        };

        if ctx.reached_max_files(&mut result) {
            break;
        }

        let descend =
            helpers::process_entry(&ctx, &entry.path, entry.file_type, entry.depth, &mut result)?;

        if let Some((dir, depth)) = descend {
            let children = helpers::read_dir_or_warn(&ctx, &dir, depth, &mut result);
            if !children.is_empty() {
                stack.push(children);
            }
        }
    }

    Ok(result)
}
