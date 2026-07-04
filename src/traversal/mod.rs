// Traversal strategy dispatch and shared helpers.
//
// Previously this module exposed a `TraversalStrategy` trait plus per-strategy
// implementations. The trait was almost free of behaviour (each strategy had
// no state) and was dispatched only once via `Box<dyn ...>`, so it has been
// replaced with plain functions:
//
//   * `traverse_single`    -- single-threaded driver used when
//     `config.thread_count == 1`. Picks DFS or BFS based on the strategy.
//   * `traverse_parallel`  -- multi-threaded driver. Uses a crossbeam
//     work-stealing deque with FIFO queues for BFS and LIFO for DFS. Each
//     worker accumulates a private `LocalResult` and the driver merges them,
//     avoiding any per-entry locking.
//
// Shared helpers used by all of the above live in `helpers`.

pub mod breadth_first;
pub mod depth_first;
pub mod helpers;
pub mod parallel;

use crate::analyzer::LocalResult;
use crate::config::{AnalyzerConfig, TraversalStrategy};
use crate::error::AnalyzerError;
use crate::link_handler::LinkHandler;
use std::sync::Arc;

/// Single-threaded entry point. Returns a `LocalResult` that the caller
/// converts into an `AnalysisResult`.
pub fn traverse_single(
    config: &AnalyzerConfig,
    link_handler: &Arc<LinkHandler>,
) -> Result<LocalResult, AnalyzerError> {
    match config.traversal_strategy {
        TraversalStrategy::DepthFirst => depth_first::traverse(config, link_handler),
        TraversalStrategy::BreadthFirst => breadth_first::traverse(config, link_handler),
    }
}

/// Multi-threaded entry point.
pub fn traverse_parallel(
    config: &AnalyzerConfig,
    link_handler: &Arc<LinkHandler>,
) -> Result<LocalResult, AnalyzerError> {
    parallel::traverse(config, link_handler)
}
