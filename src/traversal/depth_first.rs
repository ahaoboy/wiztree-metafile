// Depth-first traversal strategy

use crate::collector::ResultCollector;
use crate::config::AnalyzerConfig;
use crate::error::AnalyzerError;
use crate::link_handler::LinkHandler;
use crate::processor::FileProcessor;
use crate::traversal::TraversalStrategy;
use crate::walker::DirectoryWalker;
use std::fs;
use std::path::Path;
use std::sync::Arc;

pub struct DepthFirstTraversal;

impl DepthFirstTraversal {
    pub fn new() -> Self {
        Self
    }

    fn traverse_recursive(
        &self,
        path: &Path,
        depth: usize,
        config: &AnalyzerConfig,
        walker: &DirectoryWalker,
        link_handler: &Arc<LinkHandler>,
        processor: &FileProcessor,
        collector: &ResultCollector,
    ) -> Result<(), AnalyzerError> {
        // Check file count limit
        if let Some(max_files) = config.max_files {
            if collector.file_count() >= max_files {
                collector.set_incomplete(true);
                return Ok(());
            }
        }

        // Check if this is a circular symlink
        let metadata = match fs::symlink_metadata(path) {
            Ok(m) => m,
            Err(e) => {
                collector.add_warning(format!("Cannot access {}: {}", path.display(), e));
                return Ok(());
            }
        };

        if metadata.is_symlink() {
            if link_handler.is_circular(path).unwrap_or(false) {
                collector.add_warning(format!("Circular symlink detected: {}", path.display()));
                return Ok(());
            }
        }

        // Mark directory as visited if it's a directory
        if metadata.is_dir() {
            if let Err(e) = link_handler.mark_visited(path) {
                collector.add_warning(format!("Failed to mark visited {}: {}", path.display(), e));
            }
            collector.increment_directory_count();
        }

        // Process file
        if metadata.is_file() || metadata.is_symlink() {
            if let Some(entry) = processor.process_file(path, depth)? {
                collector.add_entry(entry);
            }
        }

        // Traverse subdirectories if this is a directory
        if metadata.is_dir() {
            let entries = match walker.read_dir(path, depth, config.max_depth) {
                Ok(e) => e,
                Err(e) => {
                    collector.add_warning(format!("Cannot read directory {}: {}", path.display(), e));
                    return Ok(());
                }
            };

            for entry in entries {
                // Check file count limit before processing each entry
                if let Some(max_files) = config.max_files {
                    if collector.file_count() >= max_files {
                        collector.set_incomplete(true);
                        return Ok(());
                    }
                }

                self.traverse_recursive(
                    &entry.path,
                    entry.depth,
                    config,
                    walker,
                    link_handler,
                    processor,
                    collector,
                )?;
            }
        }

        Ok(())
    }
}

impl TraversalStrategy for DepthFirstTraversal {
    fn traverse(
        &self,
        root: &Path,
        config: &AnalyzerConfig,
        walker: &DirectoryWalker,
        link_handler: &Arc<LinkHandler>,
        collector: &ResultCollector,
    ) -> Result<(), AnalyzerError> {
        let processor = FileProcessor::new(Arc::new(config.clone()), link_handler.clone());
        self.traverse_recursive(root, 1, config, walker, link_handler, &processor, collector)
    }
}
