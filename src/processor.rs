// File processing and size calculation

use crate::analyzer::FileEntry;
use crate::config::AnalyzerConfig;
use crate::error::AnalyzerError;
use crate::link_handler::LinkHandler;
use std::fs;
use std::path::Path;
use std::sync::Arc;

pub struct FileProcessor {
    config: Arc<AnalyzerConfig>,
    link_handler: Arc<LinkHandler>,
}

impl FileProcessor {
    pub fn new(config: Arc<AnalyzerConfig>, link_handler: Arc<LinkHandler>) -> Self {
        Self {
            config,
            link_handler,
        }
    }

    /// Process a file and return a FileEntry if it should be included
    pub fn process_file(
        &self,
        path: &Path,
        depth: usize,
    ) -> Result<Option<FileEntry>, AnalyzerError> {
        // Get metadata (follow symlinks for size)
        let symlink_metadata = fs::symlink_metadata(path)?;
        let is_symlink = symlink_metadata.is_symlink();

        // For symlinks, check if it's a duplicate
        if is_symlink
            && self.link_handler.is_duplicate_inode(&symlink_metadata) {
                // Skip duplicate symlinks
                return Ok(None);
            }

        // Get the actual file metadata (following symlinks)
        let metadata = match fs::metadata(path) {
            Ok(m) => m,
            Err(_) => {
                // Broken symlink or inaccessible file
                return Ok(None);
            }
        };

        // Only process regular files
        if !metadata.is_file() {
            return Ok(None);
        }

        let size = metadata.len();

        // Apply size filter
        if !self.should_include(size) {
            return Ok(None);
        }

        // Check for duplicate inode (hard links)
        if !is_symlink && self.link_handler.is_duplicate_inode(&metadata) {
            // Skip duplicate hard links
            return Ok(None);
        }

        // Resolve symlink target if applicable
        let target = if is_symlink {
            self.link_handler.resolve_link(path).ok()
        } else {
            None
        };

        Ok(Some(FileEntry {
            path: path.to_path_buf(),
            size,
            depth,
            is_symlink,
            target,
        }))
    }

    /// Check if a file should be included based on size filter
    pub fn should_include(&self, size: u64) -> bool {
        size >= self.config.min_file_size
    }
}
