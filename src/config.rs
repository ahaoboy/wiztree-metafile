// Configuration structures for file analysis

use crate::error::AnalyzerError;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AnalyzerConfig {
    pub max_depth: Option<usize>,
    pub max_files: Option<usize>,
    pub traversal_strategy: TraversalStrategy,
    pub min_file_size: u64,
    pub thread_count: usize,
    pub output_path: Option<PathBuf>,
    pub root_path: PathBuf,
    pub ignore_patterns: Option<GlobSet>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
pub enum TraversalStrategy {
    #[default]
    DepthFirst,
    BreadthFirst,
}


impl std::str::FromStr for TraversalStrategy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "depth-first" | "dfs" | "depth" => Ok(TraversalStrategy::DepthFirst),
            "breadth-first" | "bfs" | "breadth" => Ok(TraversalStrategy::BreadthFirst),
            _ => Err(format!("Invalid traversal strategy: {}", s)),
        }
    }
}

impl AnalyzerConfig {
    pub fn new(root_path: PathBuf) -> Self {
        Self {
            max_depth: None,
            max_files: None,
            traversal_strategy: TraversalStrategy::default(),
            min_file_size: 0,
            thread_count: num_cpus::get(),
            output_path: None,
            root_path,
            ignore_patterns: None,
        }
    }

    /// Set ignore patterns from a list of glob patterns
    pub fn set_ignore_patterns(&mut self, patterns: Vec<String>) -> Result<(), AnalyzerError> {
        if patterns.is_empty() {
            self.ignore_patterns = None;
            return Ok(());
        }

        let mut builder = GlobSetBuilder::new();
        for pattern in patterns {
            let glob = Glob::new(&pattern).map_err(|e| {
                AnalyzerError::InvalidConfig(format!("Invalid glob pattern '{}': {}", pattern, e))
            })?;
            builder.add(glob);
        }

        self.ignore_patterns = Some(builder.build().map_err(|e| {
            AnalyzerError::InvalidConfig(format!("Failed to build glob set: {}", e))
        })?);

        Ok(())
    }

    /// Check if a path should be ignored
    pub fn should_ignore(&self, path: &std::path::Path) -> bool {
        if let Some(ref patterns) = self.ignore_patterns {
            patterns.is_match(path)
        } else {
            false
        }
    }

    /// Validate the configuration and return errors if invalid
    pub fn validate(&self) -> Result<(), AnalyzerError> {
        // Validate root path exists
        if !self.root_path.exists() {
            return Err(AnalyzerError::InvalidConfig(format!(
                "Root path does not exist: {}",
                self.root_path.display()
            )));
        }

        // Validate root path is accessible
        if !self.root_path.is_dir() {
            return Err(AnalyzerError::InvalidConfig(format!(
                "Root path is not a directory: {}",
                self.root_path.display()
            )));
        }

        // Validate depth is positive if specified
        if let Some(depth) = self.max_depth
            && depth == 0 {
                return Err(AnalyzerError::InvalidConfig(
                    "Maximum depth must be at least 1".to_string(),
                ));
            }

        // Validate thread count is within valid range
        let cpu_count = num_cpus::get();
        if self.thread_count == 0 {
            return Err(AnalyzerError::InvalidConfig(
                "Thread count must be at least 1".to_string(),
            ));
        }
        if self.thread_count > cpu_count {
            return Err(AnalyzerError::InvalidConfig(format!(
                "Thread count {} exceeds CPU count {}",
                self.thread_count, cpu_count
            )));
        }

        Ok(())
    }

    /// Clamp thread count to valid range (1 to CPU count)
    pub fn clamp_thread_count(&mut self) {
        let cpu_count = num_cpus::get();
        self.thread_count = self.thread_count.clamp(1, cpu_count);
    }
}
