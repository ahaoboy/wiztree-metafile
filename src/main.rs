// CLI entry point

use clap::Parser;
use file_analyzer::{AnalyzerConfig, FileAnalyzer, TraversalStrategy};
use std::path::PathBuf;
use std::process;

#[derive(Parser)]
#[command(name = "file-analyzer")]
#[command(version = "0.1.0")]
#[command(about = "Analyze directory structures and file information", long_about = None)]
struct Cli {
    /// Root directory to analyze
    #[arg(value_name = "PATH")]
    root: PathBuf,

    /// Maximum depth to traverse (1 to system max)
    #[arg(short = 'd', long = "max-depth")]
    max_depth: Option<usize>,

    /// Maximum number of files to process
    #[arg(short = 'n', long = "max-files")]
    max_files: Option<usize>,

    /// Traversal strategy: depth-first, breadth-first, dfs, bfs
    #[arg(short = 's', long = "strategy", default_value = "depth-first")]
    strategy: String,

    /// Minimum file size in bytes
    #[arg(short = 'm', long = "min-size", default_value = "0")]
    min_size: u64,

    /// Number of threads (1 to CPU count)
    #[arg(short = 't', long = "threads")]
    threads: Option<usize>,

    /// Output file path
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,

    /// Output format: text, json, metafile
    #[arg(short = 'f', long = "format", default_value = "metafile")]
    format: String,
}

fn main() {
    let cli = Cli::parse();

    // Parse traversal strategy
    let strategy = match cli.strategy.parse::<TraversalStrategy>() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    };

    // Build configuration
    let mut config = AnalyzerConfig::new(cli.root);
    config.max_depth = cli.max_depth;
    config.max_files = cli.max_files;
    config.traversal_strategy = strategy;
    config.min_file_size = cli.min_size;
    config.output_path = cli.output.clone();

    // Set thread count
    if let Some(threads) = cli.threads {
        config.thread_count = threads;
        config.clamp_thread_count();
    }

    // Parse output format
    let output_format = match cli.format.to_lowercase().as_str() {
        "text" => file_analyzer::output::OutputFormat::Text,
        "json" => file_analyzer::output::OutputFormat::Json,
        "metafile" | "meta" => file_analyzer::output::OutputFormat::Metafile,
        _ => {
            eprintln!("Error: Invalid format '{}'. Use: text, json, or metafile", cli.format);
            process::exit(1);
        }
    };

    // Run analysis
    let analyzer = FileAnalyzer::new(config);
    match analyzer.analyze() {
        Ok(result) => {
            // Write output
            if let Err(e) = file_analyzer::output::OutputWriter::write(
                &result,
                cli.output.as_deref(),
                output_format,
            ) {
                eprintln!("Error writing output: {}", e);
                process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    }
}
