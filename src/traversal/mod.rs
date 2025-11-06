// Traversal strategy trait and implementations

use crate::collector::ResultCollector;
use crate::config::AnalyzerConfig;
use crate::error::AnalyzerError;
use crate::link_handler::LinkHandler;
use crate::walker::DirectoryWalker;
use std::path::Path;
use std::sync::Arc;

pub mod breadth_first;
pub mod depth_first;

pub use breadth_first::BreadthFirstTraversal;
pub use depth_first::DepthFirstTraversal;

/// Trait for different directory traversal strategies
pub trait TraversalStrategy: Send + Sync {
    fn traverse(
        &self,
        root: &Path,
        config: &AnalyzerConfig,
        walker: &DirectoryWalker,
        link_handler: &Arc<LinkHandler>,
        collector: &ResultCollector,
    ) -> Result<(), AnalyzerError>;
}
