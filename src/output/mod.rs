// Output formatters

use crate::analyzer::AnalysisResult;
use crate::error::AnalyzerError;
use std::fs::File;
use std::io::Write;
use std::path::Path;

pub mod json;
pub mod metafile;
pub mod text;

pub use json::JsonFormatter;
pub use metafile::MetafileFormatter;
pub use text::TextFormatter;

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
    pub fn write(
        result: &AnalysisResult,
        output_path: Option<&Path>,
        format: OutputFormat,
    ) -> Result<(), AnalyzerError> {
        match output_path {
            Some(path) => {
                // Write to file with specified format
                let content = match format {
                    OutputFormat::Text => {
                        let formatter = TextFormatter;
                        formatter.format(result)?
                    }
                    OutputFormat::Json => {
                        let formatter = JsonFormatter;
                        formatter.format(result)?
                    }
                    OutputFormat::Metafile => {
                        let formatter = MetafileFormatter;
                        formatter.format(result)?
                    }
                };
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
