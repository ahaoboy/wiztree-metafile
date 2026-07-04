// Core file analyzer orchestration.
//
// Holds the public types (`FileEntry`, `AnalysisResult`) and a thin
// `FileAnalyzer` driver that dispatches to either the single-threaded or
// parallel traversal implementation in `crate::traversal`.
//
// `LocalResult` is the per-traversal accumulator used by the traversals. Each
// traversal thread/process builds its own `LocalResult` (a plain `Vec` of
// entries, no locking, no atomics); the parallel driver merges them at the
// end. This replaces the old `ResultCollector`, which used a `Mutex<Vec>` on
// the hot path and serialised every file insertion across threads.

use crate::config::AnalyzerConfig;
use crate::error::AnalyzerError;
use crate::link_handler::LinkHandler;
use crate::traversal;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: PathBuf,
    pub size: u64,
    pub depth: usize,
    pub is_symlink: bool,
    pub target: Option<PathBuf>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub total_size: u64,
    pub file_count: usize,
    pub directory_count: usize,
    pub symlink_count: usize,
    pub entries: Vec<FileEntry>,
    pub warnings: Vec<String>,
    pub incomplete: bool,
}

/// Per-traversal accumulator. Designed to be obtained cheaply per logical
/// "task" (one directory subtree in DFS, one layer slice in BFS, one
/// worker in the parallel traversal) and merged at the end without locking.
#[derive(Debug, Default, Clone)]
pub struct LocalResult {
    pub entries: Vec<FileEntry>,
    pub warnings: Vec<String>,
    pub total_size: u64,
    pub file_count: usize,
    pub directory_count: usize,
    pub symlink_count: usize,
    pub incomplete: bool,
}

impl LocalResult {
    pub fn add_entry(&mut self, entry: FileEntry) {
        self.total_size += entry.size;
        self.file_count += 1;
        if entry.is_symlink {
            self.symlink_count += 1;
        }
        self.entries.push(entry);
    }

    pub fn add_warning<S: Into<String>>(&mut self, warning: S) {
        self.warnings.push(warning.into());
    }

    #[inline]
    pub fn inc_dir(&mut self) {
        self.directory_count += 1;
    }

    /// Merge another `LocalResult` into this one. Consumes `other`.
    pub fn merge(&mut self, other: LocalResult) {
        self.total_size += other.total_size;
        self.file_count += other.file_count;
        self.directory_count += other.directory_count;
        self.symlink_count += other.symlink_count;
        self.incomplete |= other.incomplete;
        self.entries.extend(other.entries);
        self.warnings.extend(other.warnings);
    }

    #[inline]
    pub fn file_count(&self) -> usize {
        self.file_count
    }
}

impl From<LocalResult> for AnalysisResult {
    fn from(l: LocalResult) -> Self {
        AnalysisResult {
            total_size: l.total_size,
            file_count: l.file_count,
            directory_count: l.directory_count,
            symlink_count: l.symlink_count,
            entries: l.entries,
            warnings: l.warnings,
            incomplete: l.incomplete,
        }
    }
}

pub struct FileAnalyzer {
    config: AnalyzerConfig,
}

impl FileAnalyzer {
    pub fn new(config: AnalyzerConfig) -> Self {
        Self { config }
    }

    pub fn analyze(&mut self) -> Result<AnalysisResult, AnalyzerError> {
        self.config.validate()?;

        let link_handler = Arc::new(LinkHandler::new());

        // Validation already guarantees thread_count >= 1, so we can safely
        // treat `1` as the single-threaded code path and everything else as
        // parallel. Parallel traversal is responsible for setting up its own
        // rayon pool with the requested number of threads.
        if self.config.thread_count == 1 {
            traversal::traverse_single(&self.config, &link_handler).map(AnalysisResult::from)
        } else {
            traversal::traverse_parallel(&self.config, &link_handler).map(AnalysisResult::from)
        }
    }
}
