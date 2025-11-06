// File Analyzer Library
// A tool for analyzing directory structures and file information
//
//! # File Analyzer
//!
//! A Rust library and CLI tool for analyzing directory structures and file information.
//! Designed to handle large file systems (100,000+ files) efficiently with configurable
//! traversal strategies, multi-threading support, and memory-efficient streaming.
//!
//! ## Features
//!
//! - **Configurable depth limits**: Control how deep to traverse directory structures
//! - **File count limits**: Prevent excessive processing time on large file systems
//! - **Multiple traversal strategies**: Choose between depth-first and breadth-first
//! - **Size filtering**: Focus on files above a minimum size threshold
//! - **Multi-threading**: Leverage multiple CPU cores for faster processing
//! - **Symbolic link handling**: Correctly handle symlinks and prevent circular references
//! - **Flexible output**: Output to stdout (text) or file (JSON)
//!
//! ## Example
//!
//! ```no_run
//! use file_analyzer::{AnalyzerConfig, FileAnalyzer, TraversalStrategy};
//! use std::path::PathBuf;
//!
//! let mut config = AnalyzerConfig::new(PathBuf::from("."));
//! config.max_depth = Some(3);
//! config.min_file_size = 1024; // Only files >= 1KB
//! config.traversal_strategy = TraversalStrategy::DepthFirst;
//!
//! let analyzer = FileAnalyzer::new(config);
//! match analyzer.analyze() {
//!     Ok(result) => {
//!         println!("Total size: {} bytes", result.total_size);
//!         println!("File count: {}", result.file_count);
//!     }
//!     Err(e) => eprintln!("Error: {}", e),
//! }
//! ```

pub mod analyzer;
pub mod collector;
pub mod config;
pub mod error;
pub mod link_handler;
pub mod output;
pub mod processor;
pub mod traversal;
pub mod walker;

// Re-export main types for convenience
pub use analyzer::{AnalysisResult, FileAnalyzer, FileEntry};
pub use config::{AnalyzerConfig, TraversalStrategy};
pub use error::AnalyzerError;
pub use output::OutputFormat;
