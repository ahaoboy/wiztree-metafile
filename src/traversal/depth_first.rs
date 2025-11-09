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

struct TraversalContext<'a> {
    config: &'a AnalyzerConfig,
    walker: &'a DirectoryWalker,
    link_handler: &'a Arc<LinkHandler>,
    processor: &'a FileProcessor,
    collector: &'a ResultCollector,
}

impl Default for DepthFirstTraversal {
    fn default() -> Self {
        Self::new()
    }
}

impl DepthFirstTraversal {
    pub fn new() -> Self {
        Self
    }

    fn traverse_recursive(
        &self,
        path: &Path,
        depth: usize,
        ctx: &TraversalContext,
    ) -> Result<(), AnalyzerError> {
        // Check if path should be ignored
        if ctx.config.should_ignore(path) {
            return Ok(());
        }

        // Check file count limit
        if let Some(max_files) = ctx.config.max_files
            && ctx.collector.file_count() >= max_files
        {
            ctx.collector.set_incomplete(true);
            return Ok(());
        }

        // Check if this is a circular symlink
        let metadata = match fs::symlink_metadata(path) {
            Ok(m) => m,
            Err(e) => {
                ctx.collector
                    .add_warning(format!("Cannot access {}: {}", path.display(), e));
                return Ok(());
            }
        };

        if metadata.is_symlink() && ctx.link_handler.is_circular(path).unwrap_or(false) {
            ctx.collector
                .add_warning(format!("Circular symlink detected: {}", path.display()));
            return Ok(());
        }

        // Mark directory as visited if it's a directory
        if metadata.is_dir() {
            if let Err(e) = ctx.link_handler.mark_visited(path) {
                ctx.collector.add_warning(format!(
                    "Failed to mark visited {}: {}",
                    path.display(),
                    e
                ));
            }
            ctx.collector.increment_directory_count();
        }

        // Process file
        if (metadata.is_file() || metadata.is_symlink())
            && let Some(entry) = ctx.processor.process_file(path, depth)?
        {
            ctx.collector.add_entry(entry);
        }

        // Traverse subdirectories if this is a directory
        if metadata.is_dir() {
            let entries = match ctx.walker.read_dir(path, depth, ctx.config.max_depth) {
                Ok(e) => e,
                Err(e) => {
                    ctx.collector.add_warning(format!(
                        "Cannot read directory {}: {}",
                        path.display(),
                        e
                    ));
                    return Ok(());
                }
            };

            for entry in entries {
                // Check file count limit before processing each entry
                if let Some(max_files) = ctx.config.max_files
                    && ctx.collector.file_count() >= max_files
                {
                    ctx.collector.set_incomplete(true);
                    return Ok(());
                }

                self.traverse_recursive(&entry.path, entry.depth, ctx)?;
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
        let ctx = TraversalContext {
            config,
            walker,
            link_handler,
            processor: &processor,
            collector,
        };
        self.traverse_recursive(root, 1, &ctx)
    }
}
