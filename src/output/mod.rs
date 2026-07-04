// Output formatters and writers.
//
// `OutputWriter::write` now uses a `BufWriter` when writing to a file, which
// is critical for performance on multi-MB JSON outputs — the previous code
// called `File::write_all(json_string.as_bytes())` directly, which performs
// one `WriteFile` syscall per `write_all` request and is dramatically slower
// than streaming through a 64KB buffer.

use crate::analyzer::AnalysisResult;
use crate::error::AnalyzerError;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

pub mod metafile;
pub use metafile::MetafileFormatter;

/// Trait for formatting analysis results.
pub trait OutputFormatter {
    /// Format the result into a string. Used for stdout output.
    fn format(&self, result: &AnalysisResult) -> Result<String, AnalyzerError>;

    /// Stream the result directly to a writer. Default implementation falls
    /// back to `format` + `write_all`; formatters with a streaming fast path
    /// should override this to avoid allocating the full JSON string.
    fn write(&self, result: &AnalysisResult, writer: &mut dyn Write) -> Result<(), AnalyzerError> {
        let s = self.format(result)?;
        writer.write_all(s.as_bytes())?;
        Ok(())
    }
}

/// Output format type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
    Metafile,
}

/// Writes analysis results to stdout or file.
pub struct OutputWriter;

impl OutputWriter {
    pub fn write(result: &AnalysisResult, output_path: Option<&Path>) -> Result<(), AnalyzerError> {
        let formatter = MetafileFormatter;
        match output_path {
            Some(path) => {
                let file = File::create(path)?;
                let mut writer = BufWriter::new(file);
                formatter.write(result, &mut writer)?;
                writer.flush()?;
            }
            None => {
                let mut stdout = std::io::stdout().lock();
                formatter.write(result, &mut stdout)?;
                stdout.flush()?;
            }
        }
        Ok(())
    }
}
