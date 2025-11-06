// Breadth-first traversal strategy

use crate::collector::ResultCollector;
use crate::config::AnalyzerConfig;
use crate::error::AnalyzerError;
use crate::link_handler::LinkHandler;
use crate::processor::FileProcessor;
use crate::traversal::TraversalStrategy;
use crate::walker::DirectoryWalker;
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct BreadthFirstTraversal;

impl BreadthFirstTraversal {
    pub fn new() -> Self {
        Self
    }
}

impl TraversalStrategy for BreadthFirstTraversal {
    fn traverse(
        &self,
        root: &Path,
        config: &AnalyzerConfig,
        walker: &DirectoryWalker,
        link_handler: &Arc<LinkHandler>,
        collector: &ResultCollector,
    ) -> Result<(), AnalyzerError> {
        let processor = FileProcessor::new(Arc::new(config.clone()), link_handler.clone());
        let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
        queue.push_back((root.to_path_buf(), 1));

        while let Some((path, depth)) = queue.pop_front() {
            // Check file count limit
            if let Some(max_files) = config.max_files {
                if collector.file_count() >= max_files {
                    collector.set_incomplete(true);
                    break;
                }
            }

            // Check if this is a circular symlink
            let metadata = match fs::symlink_metadata(&path) {
                Ok(m) => m,
                Err(e) => {
                    collector.add_warning(format!("Cannot access {}: {}", path.display(), e));
                    continue;
                }
            };

            if metadata.is_symlink() {
                if link_handler.is_circular(&path).unwrap_or(false) {
                    collector.add_warning(format!("Circular symlink detected: {}", path.display()));
                    continue;
                }
            }

            // Mark directory as visited if it's a directory
            if metadata.is_dir() {
                if let Err(e) = link_handler.mark_visited(&path) {
                    collector.add_warning(format!("Failed to mark visited {}: {}", path.display(), e));
                }
                collector.increment_directory_count();
            }

            // Process file
            if metadata.is_file() || metadata.is_symlink() {
                if let Some(entry) = processor.process_file(&path, depth)? {
                    collector.add_entry(entry);
                }
            }

            // Add subdirectories to queue if this is a directory
            if metadata.is_dir() {
                let entries = match walker.read_dir(&path, depth, config.max_depth) {
                    Ok(e) => e,
                    Err(e) => {
                        collector.add_warning(format!("Cannot read directory {}: {}", path.display(), e));
                        continue;
                    }
                };

                for entry in entries {
                    queue.push_back((entry.path, entry.depth));
                }
            }
        }

        Ok(())
    }
}
