// Thread-safe result aggregation

use crate::analyzer::{AnalysisResult, FileEntry};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

pub struct ResultCollector {
    entries: Arc<Mutex<Vec<FileEntry>>>,
    warnings: Arc<Mutex<Vec<String>>>,
    total_size: Arc<AtomicU64>,
    file_count: Arc<AtomicUsize>,
    directory_count: Arc<AtomicUsize>,
    symlink_count: Arc<AtomicUsize>,
    incomplete: Arc<AtomicBool>,
}

impl Default for ResultCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl ResultCollector {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(Mutex::new(Vec::new())),
            warnings: Arc::new(Mutex::new(Vec::new())),
            total_size: Arc::new(AtomicU64::new(0)),
            file_count: Arc::new(AtomicUsize::new(0)),
            directory_count: Arc::new(AtomicUsize::new(0)),
            symlink_count: Arc::new(AtomicUsize::new(0)),
            incomplete: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Add a file entry to the results
    pub fn add_entry(&self, entry: FileEntry) {
        // Update counters
        self.total_size.fetch_add(entry.size, Ordering::Relaxed);
        self.file_count.fetch_add(1, Ordering::Relaxed);
        if entry.is_symlink {
            self.symlink_count.fetch_add(1, Ordering::Relaxed);
        }

        // Add to entries list
        let mut entries = self.entries.lock().unwrap();
        entries.push(entry);
    }

    /// Add a warning message
    pub fn add_warning(&self, warning: String) {
        let mut warnings = self.warnings.lock().unwrap();
        warnings.push(warning);
    }

    /// Increment directory count
    pub fn increment_directory_count(&self) {
        self.directory_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Get current file count
    pub fn file_count(&self) -> usize {
        self.file_count.load(Ordering::Relaxed)
    }

    /// Set incomplete flag
    pub fn set_incomplete(&self, incomplete: bool) {
        self.incomplete.store(incomplete, Ordering::Relaxed);
    }

    /// Finalize and return the analysis result
    pub fn finalize(self) -> AnalysisResult {
        let entries = match Arc::try_unwrap(self.entries) {
            Ok(mutex) => mutex.into_inner().unwrap(),
            Err(arc) => arc.lock().unwrap().clone(),
        };

        let warnings = match Arc::try_unwrap(self.warnings) {
            Ok(mutex) => mutex.into_inner().unwrap(),
            Err(arc) => arc.lock().unwrap().clone(),
        };

        AnalysisResult {
            total_size: self.total_size.load(Ordering::Relaxed),
            file_count: self.file_count.load(Ordering::Relaxed),
            directory_count: self.directory_count.load(Ordering::Relaxed),
            symlink_count: self.symlink_count.load(Ordering::Relaxed),
            entries,
            warnings,
            incomplete: self.incomplete.load(Ordering::Relaxed),
        }
    }
}
