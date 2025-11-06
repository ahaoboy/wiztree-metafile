// Directory walking logic

use crate::error::AnalyzerError;
use crate::link_handler::LinkHandler;
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Custom directory entry with depth information
#[derive(Debug)]
pub struct DirEntry {
    pub path: PathBuf,
    pub metadata: Metadata,
    pub depth: usize,
}

/// Handles directory traversal with depth tracking
pub struct DirectoryWalker {
    link_handler: Arc<LinkHandler>,
}

impl DirectoryWalker {
    pub fn new(link_handler: Arc<LinkHandler>) -> Self {
        Self { link_handler }
    }

    /// Read directory entries at the given path
    pub fn read_dir(
        &self,
        path: &Path,
        current_depth: usize,
        max_depth: Option<usize>,
    ) -> Result<Vec<DirEntry>, AnalyzerError> {
        // Check if we should traverse this directory
        if !self.should_traverse_depth(current_depth, max_depth) {
            return Ok(Vec::new());
        }

        let mut entries = Vec::new();

        // Read directory entries
        let dir_entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(e) => {
                // Return empty vec for permission denied or other errors
                // The caller can log this as a warning
                return Err(AnalyzerError::Io(e));
            }
        };

        for entry in dir_entries {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue, // Skip entries we can't read
            };

            let path = entry.path();
            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue, // Skip entries we can't get metadata for
            };

            entries.push(DirEntry {
                path,
                metadata,
                depth: current_depth + 1,
            });
        }

        Ok(entries)
    }

    /// Check if we should traverse to the next depth level
    pub fn should_traverse(&self, entry: &DirEntry, max_depth: Option<usize>) -> bool {
        if !entry.metadata.is_dir() {
            return false;
        }

        self.should_traverse_depth(entry.depth, max_depth)
    }

    /// Check if the current depth allows traversal
    fn should_traverse_depth(&self, current_depth: usize, max_depth: Option<usize>) -> bool {
        match max_depth {
            Some(max) => current_depth <= max,
            None => true,
        }
    }

    /// Check if a path is a symbolic link and handle circular references
    pub fn check_symlink(&self, path: &Path) -> Result<bool, AnalyzerError> {
        let metadata = fs::symlink_metadata(path)?;

        if metadata.is_symlink() {
            // Check for circular reference
            if self.link_handler.is_circular(path)? {
                return Ok(true); // It's a circular link
            }
        }

        Ok(false)
    }
}
