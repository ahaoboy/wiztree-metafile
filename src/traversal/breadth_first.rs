// Breadth-first traversal strategy (iterative).
//
// Uses a single `VecDeque` of pending directory entries; entries are popped
// from the front and a directory's children are pushed onto the back. This is
// the classic BFS formulation. A `VecDeque<Pair>` is used in preference to
// the original `(PathBuf, usize)` queue because the entry's `FileType` is
// already known from the listing — we don't need a second stat here.

use crate::analyzer::LocalResult;
use crate::config::AnalyzerConfig;
use crate::error::AnalyzerError;
use crate::link_handler::LinkHandler;
use crate::traversal::helpers::{self, TraverseCtx};
use crate::walker::DirEntry;
use std::collections::VecDeque;
use std::sync::Arc;

pub fn traverse(
    config: &AnalyzerConfig,
    link_handler: &Arc<LinkHandler>,
) -> Result<LocalResult, AnalyzerError> {
    let ctx = TraverseCtx::new(config, link_handler);
    let mut result = LocalResult::default();
    helpers::bootstrap_root(&ctx, &config.root_path, &mut result)?;

    let root_entries = helpers::read_dir_or_warn(&ctx, &config.root_path, 1, &mut result);

    let mut queue: VecDeque<DirEntry> = VecDeque::with_capacity(64);
    queue.extend(root_entries);

    while let Some(entry) = queue.pop_front() {
        if ctx.reached_max_files(&mut result) {
            break;
        }

        let descend =
            helpers::process_entry(&ctx, &entry.path, entry.file_type, entry.depth, &mut result)?;

        if let Some((dir, depth)) = descend {
            let children = helpers::read_dir_or_warn(&ctx, &dir, depth, &mut result);
            queue.extend(children);
        }
    }

    Ok(result)
}
