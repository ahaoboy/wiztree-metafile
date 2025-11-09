// Output formatters

use crate::analyzer::AnalysisResult;
use crate::error::AnalyzerError;
use std::fs::File;
use std::io::Write;
use std::path::Path;
pub mod metafile;
pub use metafile::MetafileFormatter;

/// Trait for formatting analysis results
pub trait OutputFormatter {
    fn format(&self, result: &AnalysisResult) -> Result<String, AnalyzerError>;
}

/// Output format type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
    Metafile,
}

/// Writes analysis results to stdout or file
pub struct OutputWriter;

impl OutputWriter {
    pub fn write(result: &AnalysisResult, output_path: Option<&Path>) -> Result<(), AnalyzerError> {
        let formatter = MetafileFormatter;
        let s = formatter.format(result)?;
        match output_path {
            Some(path) => {
                let mut file = File::create(path)?;
                file.write_all(s.as_bytes())?;
            }
            _ => {
                println!("{s}");
            }
        }
        Ok(())
    }
}
