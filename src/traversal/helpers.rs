// Shared helpers used by both single-threaded and parallel traversals.
//
// Centralizing the per-entry "process this directory and emit warnings /
// entries into a LocalResult" logic avoids the previous near-duplicated code
// in `breadth_first.rs` and `depth_first.rs` and guarantees the same behavior
// (warnings emitted with identical strings, dedup ordering, etc) across all
// traversal strategies.

use crate::analyzer::LocalResult;
use crate::config::AnalyzerConfig;
use crate::error::AnalyzerError;
use crate::link_handler::{LinkHandler, VisitOutcome};
use crate::processor::FileProcessor;
use crate::walker::DirectoryWalker;
use std::fs;
use std::path::Path;

/// Configuration cap captured at the start of each traversal. `max_files` is
/// read once and stored locally so the hot loop does not need to read it via
/// a reference on each iteration.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_depth: Option<usize>,
    pub max_files: Option<usize>,
}

impl Limits {
    pub fn from_config(config: &AnalyzerConfig) -> Self {
        Self {
            max_depth: config.max_depth,
            max_files: config.max_files,
        }
    }
}

/// Bundle everything a traversal step needs. Keeps the public entry-point
/// functions readable and avoids repeating long parameter lists.
pub struct TraverseCtx<'a> {
    pub config: &'a AnalyzerConfig,
    pub link_handler: &'a LinkHandler,
    pub walker: DirectoryWalker,
    pub processor: FileProcessor<'a>,
    pub limits: Limits,
}

impl<'a> TraverseCtx<'a> {
    pub fn new(config: &'a AnalyzerConfig, link_handler: &'a LinkHandler) -> Self {
        Self {
            config,
            link_handler,
            walker: DirectoryWalker::new(),
            processor: FileProcessor::new(config, link_handler),
            limits: Limits::from_config(config),
        }
    }

    /// Whether we should keep descending into directories at `depth`.
    #[inline]
    pub fn within_depth(&self, depth: usize, max_depth: Option<usize>) -> bool {
        match max_depth {
            Some(max) => depth <= max,
            None => true,
        }
    }

    /// Returns `true` if `result.file_count()` has reached the configured
    /// `max_files` cap; sets `incomplete` when so.
    #[inline]
    pub fn reached_max_files(&self, result: &mut LocalResult) -> bool {
        if let Some(max) = self.limits.max_files
            && result.file_count() >= max
        {
            result.incomplete = true;
            return true;
        }
        false
    }
}

/// Process a single entry that came from `walker::read_dir`.
///
/// Side effects:
///  * Appends an `FileEntry` to `result.entries` when the entry is a regular
///    file or a file-symlink that survives size + inode filtering.
///  * Returns the same entry as `(path, depth)` to indicate it should be
///    descended into (its `file_type` claimed it is a directory and it was
///    newly visited).
///
/// `result` is mutated in place; warnings are emitted via `result.add_warning`.
pub fn process_entry(
    ctx: &TraverseCtx,
    entry_path: &Path,
    entry_ft: fs::FileType,
    depth: usize,
    result: &mut LocalResult,
) -> Result<Option<(std::path::PathBuf, usize)>, AnalyzerError> {
    // Honor ignore patterns using the normalized string form so that
    // patterns written with forward slashes (as documented) match on Windows
    // as well as Unix.
    if ctx.config.should_ignore(entry_path) {
        return Ok(None);
    }

    if entry_ft.is_dir() {
        // Symlink-circular detection happens via `visit_dir`'s canonicalize
        // check. If `visit_dir` reports `AlreadyVisited`, we skip and emit a
        // warning to surface the cycle to the user.
        match ctx.link_handler.visit_dir(entry_path) {
            Ok(VisitOutcome::NewlyVisited) => {
                result.inc_dir();
                // Only return this as a directory to descend when the depth
                // limit allows it. Otherwise we still counted it as a dir,
                // but we don't return it for recursion.
                if ctx.within_depth(depth, ctx.limits.max_depth) {
                    return Ok(Some((entry_path.to_path_buf(), depth)));
                }
                Ok(None)
            }
            Ok(VisitOutcome::AlreadyVisited) => {
                result.add_warning(format!(
                    "Circular or already-visited directory detected: {}",
                    entry_path.display()
                ));
                Ok(None)
            }
            Err(e) => {
                result.add_warning(format!(
                    "Cannot canonicalize directory {}: {}",
                    entry_path.display(),
                    e
                ));
                Ok(None)
            }
        }
    } else if entry_ft.is_symlink() {
        // Detect circular symlinks BEFORE attempting to follow them: this
        // keeps the original "Circular symlink detected: <path>" warning that
        // the parallel traversal also relies upon. `is_visited` returning
        // `false` simply means the resolved target is not a directory we
        // have already been inside — fall through to the processor, which
        // resolves the symlink target itself and decides whether to count
        // it (file symlink) or skip it (dir symlink, broken link, etc).
        match ctx.link_handler.is_visited(entry_path) {
            Ok(true) => {
                result.add_warning(format!(
                    "Circular symlink detected: {}",
                    entry_path.display()
                ));
                Ok(None)
            }
            Ok(false) => match ctx.processor.process_file(entry_path, depth, true) {
                Ok(Some(entry)) => {
                    result.add_entry(entry);
                    Ok(None)
                }
                Ok(None) => Ok(None),
                Err(e) => {
                    result.add_warning(format!(
                        "Cannot process file {}: {}",
                        entry_path.display(),
                        e
                    ));
                    Ok(None)
                }
            },
            Err(_) => {
                // canonicalize failure on the symlink (e.g. broken link).
                // Just defer to the processor; it handles broken links
                // gracefully (returns Ok(None)).
                match ctx.processor.process_file(entry_path, depth, true) {
                    Ok(Some(entry)) => {
                        result.add_entry(entry);
                        Ok(None)
                    }
                    Ok(None) => Ok(None),
                    Err(e) => {
                        result.add_warning(format!(
                            "Cannot process file {}: {}",
                            entry_path.display(),
                            e
                        ));
                        Ok(None)
                    }
                }
            }
        }
    } else if entry_ft.is_file() {
        match ctx.processor.process_file(entry_path, depth, false) {
            Ok(Some(entry)) => {
                result.add_entry(entry);
                Ok(None)
            }
            Ok(None) => Ok(None),
            Err(e) => {
                result.add_warning(format!(
                    "Cannot process file {}: {}",
                    entry_path.display(),
                    e
                ));
                Ok(None)
            }
        }
    } else {
        // Sockets, fifos, block/char devices, etc. are not counted.
        Ok(None)
    }
}

/// Read the children of `dir` and accumulate warnings on error.
pub fn read_dir_or_warn(
    ctx: &TraverseCtx,
    dir: &Path,
    depth: usize,
    result: &mut LocalResult,
) -> Vec<crate::walker::DirEntry> {
    match ctx.walker.read_dir(dir, depth, ctx.limits.max_depth) {
        Ok(entries) => entries,
        Err(e) => {
            result.add_warning(format!("Cannot read directory {}: {}", dir.display(), e));
            Vec::new()
        }
    }
}

/// Process the root directory itself. Returns the `LocalResult` ready for the
/// traversal-specific top loop. The first canonicalize happens here; the
/// root counts as one directory.
pub fn bootstrap_root(
    ctx: &TraverseCtx,
    root: &Path,
    result: &mut LocalResult,
) -> Result<(), AnalyzerError> {
    if ctx.config.should_ignore(root) {
        return Ok(());
    }
    match ctx.link_handler.visit_dir(root) {
        Ok(VisitOutcome::NewlyVisited) => {
            result.inc_dir();
            Ok(())
        }
        Ok(VisitOutcome::AlreadyVisited) => Ok(()),
        Err(e) => Err(e),
    }
}
