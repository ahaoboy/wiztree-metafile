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

/// Serialization format for output files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// JSON (metafile format), human-readable, compatible with esbuild analyzer.
    Json,
    /// Binary (rkyv), compact and fast, suitable for very large outputs.
    Binary,
}

impl Format {
    /// Detect format from a file path's extension.
    /// `.json` → Json, everything else → Binary.
    pub fn from_extension(path: &Path) -> Self {
        match path.extension().and_then(|e| e.to_str()) {
            Some("json") => Format::Json,
            _ => Format::Binary,
        }
    }
}

impl std::str::FromStr for Format {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "json" => Ok(Format::Json),
            "binary" | "bin" | "rkyv" => Ok(Format::Binary),
            _ => Err(format!(
                "Invalid format '{}'. Valid options: json, binary",
                s
            )),
        }
    }
}

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

/// Writes analysis results to stdout or file.
pub struct OutputWriter;

impl OutputWriter {
    /// Write analysis results, auto-detecting format from file extension
    /// when `format` is `None`. Uses the provided `format` when `Some`.
    pub fn write(
        result: &AnalysisResult,
        output_path: Option<&Path>,
        format: Option<Format>,
    ) -> Result<(), AnalyzerError> {
        let fmt = match format {
            Some(f) => f,
            None => match output_path {
                Some(path) => Format::from_extension(path),
                None => Format::Json,
            },
        };

        match fmt {
            Format::Json => {
                let json = MetafileFormatter.format(result)?;
                const V8_STRING_LIMIT: usize = 0x1fff_ffe8; // ~512 MB
                if json.len() > V8_STRING_LIMIT {
                    eprintln!(
                        "Warning: JSON file is {}, exceeds V8's maximum \
                         string length. Consider using --format binary or a \
                         non-.json file extension for binary (rkyv) output.",
                        humansize::format_size(json.len(), humansize::DECIMAL),
                    );
                }
                match output_path {
                    Some(path) => {
                        let file = File::create(path)?;
                        let mut writer = BufWriter::new(file);
                        writer.write_all(json.as_bytes())?;
                        writer.flush()?;
                    }
                    None => {
                        let mut stdout = std::io::stdout().lock();
                        stdout.write_all(json.as_bytes())?;
                        stdout.flush()?;
                    }
                }
            }
            Format::Binary => {
                use serde_metafile::BinaryMetafile;
                let (metafile, _) = MetafileFormatter::build_metafile(result);
                let binary = BinaryMetafile::from(&metafile);
                let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&binary).map_err(|e| {
                    AnalyzerError::BinarySerialization(format!("rkyv serialization failed: {}", e))
                })?;
                match output_path {
                    Some(path) => {
                        let file = File::create(path)?;
                        let mut writer = BufWriter::new(file);
                        writer.write_all(&bytes)?;
                        writer.flush()?;
                    }
                    None => {
                        let mut stdout = std::io::stdout().lock();
                        stdout.write_all(&bytes)?;
                        stdout.flush()?;
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::{AnalysisResult, FileEntry};
    use std::path::PathBuf;

    #[test]
    fn rkyv_roundtrip() {
        let mut result = AnalysisResult {
            total_size: 1024,
            file_count: 2,
            ..Default::default()
        };
        result.entries.push(FileEntry {
            path: PathBuf::from("foo/bar.txt"),
            size: 512,
            depth: 1,
            is_symlink: false,
            target: None,
        });
        result.entries.push(FileEntry {
            path: PathBuf::from("baz/qux.rs"),
            size: 512,
            depth: 2,
            is_symlink: true,
            target: Some(PathBuf::from("real/path.rs")),
        });

        let (metafile, _) = MetafileFormatter::build_metafile(&result);
        use serde_metafile::BinaryMetafile;
        let binary = BinaryMetafile::from(&metafile);
        let bytes =
            rkyv::to_bytes::<rkyv::rancor::Error>(&binary).expect("serialization should succeed");

        assert!(!bytes.is_empty(), "serialized bytes should not be empty");

        // Round-trip: deserialize and verify
        let archived = rkyv::access::<rkyv::Archived<BinaryMetafile>, rkyv::rancor::Error>(&bytes)
            .expect("deserialization should succeed");
        let binary: BinaryMetafile =
            rkyv::deserialize::<BinaryMetafile, rkyv::rancor::Error>(archived)
                .expect("deserialization should succeed");
        let back = serde_metafile::Metafile::from(binary);
        assert_eq!(back.outputs.len(), 1);
        let output = back.outputs.values().next().unwrap();
        assert_eq!(output.bytes, 1024u64);
    }
}
