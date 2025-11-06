// Core file analyzer orchestration

use crate::collector::ResultCollector;
use crate::config::{AnalyzerConfig, TraversalStrategy};
use crate::error::AnalyzerError;
use crate::link_handler::LinkHandler;
use crate::traversal::{
    BreadthFirstTraversal, DepthFirstTraversal, TraversalStrategy as TraversalStrategyTrait,
};
use crate::walker::DirectoryWalker;
use rayon::ThreadPoolBuilder;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: PathBuf,
    pub size: u64,
    pub depth: usize,
    pub is_symlink: bool,
    pub target: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub total_size: u64,
    pub file_count: usize,
    pub directory_count: usize,
    pub symlink_count: usize,
    pub entries: Vec<FileEntry>,
    pub warnings: Vec<String>,
    pub incomplete: bool,
}

pub struct FileAnalyzer {
    config: AnalyzerConfig,
}

impl FileAnalyzer {
    pub fn new(config: AnalyzerConfig) -> Self {
        Self { config }
    }

    pub fn analyze(&self) -> Result<AnalysisResult, AnalyzerError> {
        // Validate configuration
        self.config.validate()?;

        // #[cfg(feature = "progress")]
        // {
        //     self.analyze_with_progress()
        // }

        // #[cfg(not(feature = "progress"))]
        // {
        // Choose between single-threaded and multi-threaded
        if self.config.thread_count == 1 {
            self.analyze_single_threaded()
        } else {
            self.analyze_multi_threaded()
        }
        // }
    }

    // #[cfg(feature = "progress")]
    // fn analyze_with_progress(&self) -> Result<AnalysisResult, AnalyzerError> {
    //     use indicatif::{ProgressBar, ProgressStyle};

    //     let pb = ProgressBar::new_spinner();
    //     pb.set_style(
    //         ProgressStyle::default_spinner()
    //             .template("{spinner:.green} [{elapsed_precise}] {msg}")
    //             .unwrap(),
    //     );
    //     pb.set_message("Analyzing files...");

    //     let result = if self.config.thread_count == 1 {
    //         self.analyze_single_threaded()
    //     } else {
    //         self.analyze_multi_threaded()
    //     };

    //     pb.finish_with_message("Analysis complete");
    //     result
    // }

    fn analyze_single_threaded(&self) -> Result<AnalysisResult, AnalyzerError> {
        let link_handler = Arc::new(LinkHandler::new());
        let walker = DirectoryWalker::new(link_handler.clone());
        let collector = ResultCollector::new();

        // Select traversal strategy
        let strategy: Box<dyn TraversalStrategyTrait> = match self.config.traversal_strategy {
            TraversalStrategy::DepthFirst => Box::new(DepthFirstTraversal::new()),
            TraversalStrategy::BreadthFirst => Box::new(BreadthFirstTraversal::new()),
        };

        // Perform traversal
        strategy.traverse(
            &self.config.root_path,
            &self.config,
            &walker,
            &link_handler,
            &collector,
        )?;

        Ok(collector.finalize())
    }

    fn analyze_multi_threaded(&self) -> Result<AnalysisResult, AnalyzerError> {
        // Build thread pool
        let pool = ThreadPoolBuilder::new()
            .num_threads(self.config.thread_count)
            .build()
            .map_err(|e| AnalyzerError::ThreadPool(e.to_string()))?;

        // For now, use single-threaded approach within the pool
        // Full multi-threaded implementation would require more complex coordination
        let result = pool.install(|| self.analyze_single_threaded())?;

        Ok(result)
    }
}
