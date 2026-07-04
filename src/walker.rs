// Directory walking logic.
//
// `read_dir` returns entries annotated with their `FileType` only. The
// `FileType` is obtained from `DirEntry::file_type()`, which on both Linux
// (via `getdents`) and Windows (via `FindFirstFile`/`FindNextFile`) is cached
// directly inside the directory entry and requires **no** extra `stat` call.
//
// File size / inode are fetched later, lazily, by `FileProcessor` only when an
// entry actually needs to be counted. This eliminates the original "one
// `entry.metadata()` per entry plus one `fs::symlink_metadata()` in the
// processor" double-stat pattern.

use crate::error::AnalyzerError;
use std::fs;
use std::path::{Path, PathBuf};

/// Custom directory entry with depth information.
///
/// Holds `file_type` instead of `Metadata` to avoid an extra syscall per
/// entry. Use `entry.file_type()` of std `fs::DirEntry` to populate it.
#[derive(Debug)]
pub struct DirEntry {
    pub path: PathBuf,
    pub file_type: fs::FileType,
    pub depth: usize,
}

/// Handles directory traversal with depth tracking.
///
/// Stateless — directory listing + the depth/ignore policy live here while
/// everything else (symlink/circular handling, inode dedup, file size
/// resolution) is delegated to `LinkHandler` and `FileProcessor`.
pub struct DirectoryWalker;

impl Default for DirectoryWalker {
    fn default() -> Self {
        Self::new()
    }
}

impl DirectoryWalker {
    pub fn new() -> Self {
        Self
    }

    /// Read directory entries at the given path.
    ///
    /// Returns an empty Vec when the caller has hit the depth limit (so
    /// `read_dir` does not even open the directory in that case, saving one
    /// syscall). IO errors are returned so the caller can emit a warning.
    pub fn read_dir(
        &self,
        path: &Path,
        current_depth: usize,
        max_depth: Option<usize>,
    ) -> Result<Vec<DirEntry>, AnalyzerError> {
        // Check depth limit _before_ opening the directory.
        if !Self::should_traverse_depth(current_depth, max_depth) {
            return Ok(Vec::new());
        }

        let dir_entries = fs::read_dir(path)?;

        let mut entries = Vec::with_capacity(16); // modest hint to avoid early reallocs
        for entry in dir_entries {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue, // Skip entries we can't read
            };
            // file_type() does not follow symlinks and is cached in the
            // directory entry on both Linux and Windows — no syscall.
            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue, // Skip entries we can't classify
            };
            entries.push(DirEntry {
                path: entry.path(),
                file_type,
                depth: current_depth + 1,
            });
        }

        Ok(entries)
    }

    /// Check if the current depth allows traversal.
    pub fn should_traverse_depth(current_depth: usize, max_depth: Option<usize>) -> bool {
        match max_depth {
            Some(max) => current_depth <= max,
            None => true,
        }
    }
}
