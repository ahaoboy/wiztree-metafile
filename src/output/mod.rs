// Output formatters

use crate::analyzer::AnalysisResult;
use crate::error::AnalyzerError;
use std::fs::File;
use std::io::Write;
use std::path::Path;

pub mod json;
pub mod text;

pub use json::JsonFormatter;
pub use text::TextFormatter;

/// Trait for formatting analysis results
pub trait OutputFormatter {
    fn format(&self, result: &AnalysisResult) -> Result<String, AnalyzerError>;
}

/// Writes analysis results to stdout or file
pub struct OutputWriter;

impl OutputWriter {
    pub fn write(
        result: &AnalysisResult,
        output_path: Option<&Path>,
    ) -> Result<(), AnalyzerError> {
        match output_path {
            Some(path) => {
                // Write to file as JSON
                let formatter = JsonFormatter;
                let content = formatter.format(result)?;
                let mut file = File::create(path)?;
                file.write_all(content.as_bytes())?;
            }
            None => {
                // Write to stdout as text
                let formatter = TextFormatter;
                let content = formatter.format(result)?;
                println!("{}", content);
            }
        }
        Ok(())
    }
}
