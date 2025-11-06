// Symbolic link detection and handling

use crate::error::AnalyzerError;
use std::collections::HashSet;
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

/// Handles symbolic link detection and circular reference prevention
pub struct LinkHandler {
    visited_inodes: Arc<Mutex<HashSet<FileId>>>,
    visited_paths: Arc<Mutex<HashSet<PathBuf>>>,
}

/// Platform-independent file identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FileId {
    #[cfg(unix)]
    dev: u64,
    #[cfg(unix)]
    ino: u64,
    #[cfg(windows)]
    volume_serial: u32,
    #[cfg(windows)]
    file_index: u64,
}

impl FileId {
    #[cfg(unix)]
    fn from_metadata(metadata: &Metadata) -> Self {
        Self {
            dev: metadata.dev(),
            ino: metadata.ino(),
        }
    }

    #[cfg(windows)]
    fn from_metadata(_metadata: &Metadata) -> Option<Self> {
        // On Windows, file index requires unstable features
        // For now, we'll disable duplicate detection on Windows
        // TODO: Implement when windows_by_handle is stabilized
        None
    }
}

impl Default for LinkHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl LinkHandler {
    pub fn new() -> Self {
        Self {
            visited_inodes: Arc::new(Mutex::new(HashSet::new())),
            visited_paths: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Check if a directory path would create a circular reference
    pub fn is_circular(&self, path: &Path) -> Result<bool, AnalyzerError> {
        let canonical = path.canonicalize().map_err(|e| {
            AnalyzerError::PathError(format!(
                "Failed to canonicalize path {}: {}",
                path.display(),
                e
            ))
        })?;

        let visited = self.visited_paths.lock().unwrap();
        Ok(visited.contains(&canonical))
    }

    /// Mark a directory path as visited to detect circular references
    pub fn mark_visited(&self, path: &Path) -> Result<(), AnalyzerError> {
        let canonical = path.canonicalize().map_err(|e| {
            AnalyzerError::PathError(format!(
                "Failed to canonicalize path {}: {}",
                path.display(),
                e
            ))
        })?;

        let mut visited = self.visited_paths.lock().unwrap();
        visited.insert(canonical);
        Ok(())
    }

    /// Check if a file has already been counted (duplicate inode)
    pub fn is_duplicate_inode(&self, metadata: &Metadata) -> bool {
        #[cfg(unix)]
        {
            let file_id = FileId::from_metadata(metadata);
            let mut visited = self.visited_inodes.lock().unwrap();
            !visited.insert(file_id)
        }

        #[cfg(windows)]
        {
            if let Some(file_id) = FileId::from_metadata(metadata) {
                let mut visited = self.visited_inodes.lock().unwrap();
                !visited.insert(file_id)
            } else {
                // If we can't get file index, assume it's not a duplicate
                false
            }
        }

        #[cfg(not(any(unix, windows)))]
        {
            // On other platforms, we can't detect duplicates
            false
        }
    }

    /// Resolve a symbolic link to its target
    pub fn resolve_link(&self, path: &Path) -> Result<PathBuf, AnalyzerError> {
        std::fs::read_link(path).map_err(|e| {
            AnalyzerError::PathError(format!(
                "Failed to resolve symlink {}: {}",
                path.display(),
                e
            ))
        })
    }
}
