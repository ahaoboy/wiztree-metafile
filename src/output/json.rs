// JSON output formatter

use crate::analyzer::AnalysisResult;
use crate::error::AnalyzerError;
use crate::output::OutputFormatter;

pub struct JsonFormatter;

impl OutputFormatter for JsonFormatter {
    fn format(&self, result: &AnalysisResult) -> Result<String, AnalyzerError> {
        let json = serde_json::to_string_pretty(result)?;
        Ok(json)
    }
}
