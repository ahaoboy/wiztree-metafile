// Symbolic link / hard link detection and circular reference prevention.
//
// Design goals:
// * A single canonicalize syscall per visited directory (the old code called
//   `canonicalize` twice: once inside `is_circular`, once inside `mark_visited`).
// * A single lock + insert per inode visit; the boolean return propagates the
//   "was this already visited?" information in one call instead of two.
// * Both sets are still protected by a `Mutex` because hard-link dedup has to
//   be observed across worker threads; for the common case (no dedup) the cost
//   is a single uncontended lock.

use crate::error::AnalyzerError;
use std::collections::HashSet;
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

/// Handles symbolic link detection and circular reference prevention.
pub struct LinkHandler {
    // On platforms without inode access (Windows today) this field is never
    // used; keep it under `allow(dead_code)` rather than `#[cfg(unix)]` so
    // the struct layout doesn't change across platforms and so the
    // `InodeOutcome::Unavailable` path can still be hand-coded above.
    #[cfg_attr(not(unix), allow(dead_code))]
    visited_inodes: Mutex<HashSet<FileId>>,
    visited_paths: Mutex<HashSet<PathBuf>>,
}

/// Platform-independent file identifier.
///
/// On Unix we use `(dev, ino)`, which is cheap (already in `Metadata`).
/// On Windows the standard library does not yet expose the volume serial +
/// file index in a stable way, so we fall back to "no dedup" for hard links.
/// Symlink circular references are still detected via `visit_dir`, which
/// canonicalizes the path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FileId {
    #[cfg(unix)]
    dev: u64,
    #[cfg(unix)]
    ino: u64,
    // On non-Unix the struct exists only so `Mutex<HashSet<FileId>>` typechecks;
    // it is never constructed or queried.
    #[cfg(not(unix))]
    _unsplit: (),
}

impl FileId {
    #[cfg(unix)]
    fn from_metadata(metadata: &Metadata) -> Self {
        Self {
            dev: metadata.dev(),
            ino: metadata.ino(),
        }
    }
}

/// Outcome of a directory visit attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisitOutcome {
    /// The directory was newly added to the visited set; the caller should
    /// descend into it and count it.
    NewlyVisited,
    /// The directory (after canonicalization) was already visited; the caller
    /// MUST skip it to avoid double-counting and infinite recursion.
    AlreadyVisited,
}

/// Outcome of an inode visit attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InodeOutcome {
    /// First time we encounter this file/inode — count it.
    NewlyVisited,
    /// Already counted via a different path (hard link or symlink target).
    /// The caller MUST skip it.
    AlreadyVisited,
    /// Inode-based dedup is unavailable on this platform; nothing was inserted.
    /// The caller may treat the file as new.
    Unavailable,
}

impl Default for LinkHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl LinkHandler {
    pub fn new() -> Self {
        Self {
            visited_inodes: Mutex::new(HashSet::new()),
            visited_paths: Mutex::new(HashSet::new()),
        }
    }

    /// Visit a directory in one shot: canonicalize the path, then atomically
    /// check-and-insert into the visited set.
    ///
    /// Returns `NewlyVisited` if the directory was unknown, `AlreadyVisited`
    /// if we have seen this canonical path before (e.g. a symlink pointing at
    /// a directory we have already descended into).
    ///
    /// Failures to canonicalize (e.g. broken symlink whose target has been
    /// deleted) are reported as errors; the caller typically logs a warning
    /// and skips the path.
    pub fn visit_dir(&self, path: &Path) -> Result<VisitOutcome, AnalyzerError> {
        let canonical = path.canonicalize().map_err(|e| {
            AnalyzerError::PathError(format!(
                "Failed to canonicalize path {}: {}",
                path.display(),
                e
            ))
        })?;

        let mut visited = self.visited_paths.lock().expect("visited_paths lock");
        if visited.insert(canonical) {
            Ok(VisitOutcome::NewlyVisited)
        } else {
            Ok(VisitOutcome::AlreadyVisited)
        }
    }

    /// Try to mark a file's inode as visited.
    ///
    /// On platforms where inode detection is available (Unix), this performs
    /// a single atomic check-and-insert and returns the outcome. On platforms
    /// without inode access (Windows today) it returns `Unavailable`, meaning
    /// the caller should proceed as if the file is new.
    pub fn visit_inode(&self, metadata: &Metadata) -> InodeOutcome {
        #[cfg(unix)]
        {
            let file_id = FileId::from_metadata(metadata);
            let mut visited = self.visited_inodes.lock().expect("visited_inodes lock");
            if visited.insert(file_id) {
                InodeOutcome::NewlyVisited
            } else {
                InodeOutcome::AlreadyVisited
            }
        }

        #[cfg(not(unix))]
        {
            let _ = metadata;
            InodeOutcome::Unavailable
        }
    }

    /// Read-only check: is `path` (after canonicalization) already in the
    /// visited set? Used to warn the user about circular symlinks without
    /// mutating the visited set (we don't want to record a directory as
    /// visited merely because a symlink happened to point at it).
    ///
    /// For a symlink-to-dir whose target we have already descended into, this
    /// returns `true` so the caller can warn and skip the symlink.
    pub fn is_visited(&self, path: &Path) -> Result<bool, AnalyzerError> {
        let canonical = path.canonicalize().map_err(|e| {
            AnalyzerError::PathError(format!(
                "Failed to canonicalize path {}: {}",
                path.display(),
                e
            ))
        })?;
        let visited = self.visited_paths.lock().expect("visited_paths lock");
        Ok(visited.contains(&canonical))
    }

    /// Resolve a symbolic link to its target. Used purely to populate the
    /// `target` field of `FileEntry`.
    pub fn resolve_link(&self, path: &Path) -> Result<PathBuf, AnalyzerError> {
        std::fs::read_link(path).map_err(|e| {
            AnalyzerError::PathError(format!(
                "Failed to resolve symlink {}: {}",
                path.display(),
                e
            ))
        })
    }

    /// Returns true if this LinkHandler can dedup files via inodes on the
    /// current platform.
    pub fn supports_inode_dedup() -> bool {
        cfg!(unix)
    }
}
