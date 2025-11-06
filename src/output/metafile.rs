// Metafile output formatter (esbuild compatible)

use crate::analyzer::AnalysisResult;
use crate::error::AnalyzerError;
use crate::output::OutputFormatter;
use serde_metafile::{Input, InputDetail, Metafile, Output};
use std::collections::HashMap;

pub struct MetafileFormatter;

impl OutputFormatter for MetafileFormatter {
    fn format(&self, result: &AnalysisResult) -> Result<String, AnalyzerError> {
        let mut inputs = HashMap::new();
        let mut outputs = HashMap::new();

        // Convert file entries to metafile inputs
        for entry in &result.entries {
            let path_str = entry.path.display().to_string();

            let input = Input {
                bytes: entry.size,
                imports: vec![],
                format: None,
                with: None,
            };

            inputs.insert(path_str.clone(), input);

            // Also create an output entry for each file
            let output = Output {
                bytes: entry.size,
                inputs: HashMap::from([(path_str.clone(), InputDetail {
                    bytes_in_output: entry.size,
                })]),
                imports: vec![],
                exports: vec![],
                entry_point: None,
                css_bundle: None,
            };

            outputs.insert(path_str, output);
        }

        let metafile = Metafile { inputs, outputs };
        let json = serde_json::to_string_pretty(&metafile)?;
        Ok(json)
    }
}
