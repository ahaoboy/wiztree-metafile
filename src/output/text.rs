// Human-readable text output formatter

use crate::analyzer::AnalysisResult;
use crate::error::AnalyzerError;
use crate::output::OutputFormatter;

pub struct TextFormatter;

impl OutputFormatter for TextFormatter {
    fn format(&self, result: &AnalysisResult) -> Result<String, AnalyzerError> {
        let mut output = String::new();

        // Summary statistics
        output.push_str("=== File Analysis Results ===\n\n");
        output.push_str(&format!("Total Size: {} bytes ({:.2} MB)\n",
            result.total_size,
            result.total_size as f64 / 1_048_576.0));
        output.push_str(&format!("File Count: {}\n", result.file_count));
        output.push_str(&format!("Directory Count: {}\n", result.directory_count));
        output.push_str(&format!("Symlink Count: {}\n", result.symlink_count));

        if result.incomplete {
            output.push_str("\n⚠ WARNING: Analysis incomplete (limits reached)\n");
        }

        // Warnings
        if !result.warnings.is_empty() {
            output.push_str(&format!("\n=== Warnings ({}) ===\n", result.warnings.len()));
            for warning in &result.warnings {
                output.push_str(&format!("  - {}\n", warning));
            }
        }

        // File entries
        if !result.entries.is_empty() {
            output.push_str(&format!("\n=== Files ({}) ===\n", result.entries.len()));
            for entry in &result.entries {
                let symlink_marker = if entry.is_symlink { " -> " } else { "" };
                let target = entry.target.as_ref()
                    .map(|t| t.display().to_string())
                    .unwrap_or_default();

                output.push_str(&format!(
                    "  [Depth {}] {} bytes: {}{}{}\n",
                    entry.depth,
                    entry.size,
                    entry.path.display(),
                    symlink_marker,
                    target
                ));
            }
        }

        Ok(output)
    }
}
