// File processing and size calculation.
//
// This module is the single place where we touch file `Metadata` for size and
// inode dedup. The previous version made up to three `stat` syscalls per file:
//  1. `entry.metadata()` inside `walker::read_dir` (followed symlinks).
//  2. `fs::symlink_metadata(path)` here.
//  3. `fs::metadata(path)` here to follow symlinks for size.
//
// The new version:
//  * No longer fetches metadata in the walker (the walker uses `file_type()`,
//    which is free on Linux and Windows).
//  * Calls `fs::symlink_metadata` exactly once per regular file (this gives us
//    both size and inode — hard links can be deduped with a single call).
//  * For symlinks we additionally follow once with `fs::metadata` to obtain
//    the target's size and inode; multiple symlinks pointing at the same
//    target are now deduped via the target inode (previously they were counted
//    once per distinct symlink, which overcounted).
//  * Broken symlinks (`fs::metadata` fails) are silently skipped.
//  * Symlinks whose target is a directory are skipped from the file list
//    because directory contents are already accounted for via traversal,
//    and descending through them risks double-counting + infinite recursion.
//
// All inode operations now go through `LinkHandler::visit_inode`, which uses
// a single `Mutex` + check-and-insert; the previous code did separate
// lock/insert via `is_duplicate_inode` plus a `read_link` call.

use crate::analyzer::FileEntry;
use crate::config::AnalyzerConfig;
use crate::error::AnalyzerError;
use crate::link_handler::{InodeOutcome, LinkHandler};
use std::fs;
use std::path::Path;

pub struct FileProcessor<'a> {
    config: &'a AnalyzerConfig,
    link_handler: &'a LinkHandler,
}

impl<'a> FileProcessor<'a> {
    pub fn new(config: &'a AnalyzerConfig, link_handler: &'a LinkHandler) -> Self {
        Self {
            config,
            link_handler,
        }
    }

    /// Returns the target's metadata of a symlink, or `None` if the link is
    /// broken or points to a non-file.
    fn symlink_target_file_metadata(path: &Path) -> Option<fs::Metadata> {
        match fs::metadata(path) {
            Ok(m) if m.is_file() => Some(m),
            _ => None,
        }
    }

    /// Process a single entry. The caller is responsible for having already
    /// classified the entry (e.g. via `walker::DirEntry::file_type`) — this
    /// function only handles regular files and symlinks.
    ///
    /// `is_symlink` -- true when the directory entry's file_type reports the
    /// path as a symlink (which is the non-following `file_type()` result, i.e.
    /// it indicates the path IS a symlink, regardless of target).
    pub fn process_file(
        &self,
        path: &Path,
        depth: usize,
        is_symlink: bool,
    ) -> Result<Option<FileEntry>, AnalyzerError> {
        // `symlink_metadata` does not follow links. For regular files it gives
        // us the file size and inode in a single syscall. For symlinks it
        // gives us the symlink's own metadata (link length, link inode).
        let symlink_meta = fs::symlink_metadata(path)?;

        if is_symlink {
            // Mark this physical symlink as visited so a second path that
            // points at the same symlink is not double-counted.
            if matches!(
                self.link_handler.visit_inode(&symlink_meta),
                InodeOutcome::AlreadyVisited
            ) {
                return Ok(None);
            }

            // Resolve the target. Symlinks to directories are not followed
            // (their contents are handled via directory traversal); broken
            // links are silently skipped.
            let target_meta = match Self::symlink_target_file_metadata(path) {
                Some(m) => m,
                None => return Ok(None),
            };

            // Dedup multiple symlinks (or a symlink plus a regular path)
            // pointing at the same target file. The target's inode is the
            // canonical identity.
            if matches!(
                self.link_handler.visit_inode(&target_meta),
                InodeOutcome::AlreadyVisited
            ) {
                return Ok(None);
            }

            let size = target_meta.len();
            if !self.should_include(size) {
                return Ok(None);
            }

            // Lazy: only stat the link itself when we actually need to record
            // the target path in the output. On Windows + Unix `read_link` is
            // a single cheap syscall.
            let target = self.link_handler.resolve_link(path).ok();

            Ok(Some(FileEntry {
                path: path.to_path_buf(),
                size,
                depth,
                is_symlink: true,
                target,
            }))
        } else {
            // Regular file. `symlink_meta` is also its `Metadata` (no extra
            // follow needed), so one stat = one inode + one size, and hard
            // links are deduped here.
            if matches!(
                self.link_handler.visit_inode(&symlink_meta),
                InodeOutcome::AlreadyVisited
            ) {
                return Ok(None);
            }

            let size = symlink_meta.len();
            if !self.should_include(size) {
                return Ok(None);
            }

            Ok(Some(FileEntry {
                path: path.to_path_buf(),
                size,
                depth,
                is_symlink: false,
                target: None,
            }))
        }
    }

    /// Check if a file should be included based on size filter.
    #[inline]
    pub fn should_include(&self, size: u64) -> bool {
        size >= self.config.min_file_size
    }
}
