// Traversal strategy trait and implementations

use crate::collector::ResultCollector;
use crate::config::AnalyzerConfig;
use crate::error::AnalyzerError;
use crate::link_handler::LinkHandler;
use crate::walker::DirectoryWalker;
use std::path::Path;
use std::sync::Arc;

pub mod depth_first;
pub mod breadth_first;

pub use depth_first::DepthFirstTraversal;
pub use breadth_first::BreadthFirstTraversal;

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
