use crate::config::{Config, InputFormat, StatsFormat};
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct RunArgs {
    #[arg(long)]
    pub config: PathBuf,
    #[arg(long = "input")]
    pub input: Vec<String>,
    #[arg(long = "input-format")]
    pub input_format: Option<String>,
    #[arg(long)]
    pub output: Option<String>,
    #[arg(long)]
    pub threads: Option<usize>,
    #[arg(long = "batch-size")]
    pub batch_size: Option<usize>,
    #[arg(long = "log-level")]
    pub log_level: Option<String>,
    #[arg(long = "stats-output")]
    pub stats_output: Option<String>,
    #[arg(long = "stats-format")]
    pub stats_format: Option<String>,
    #[arg(long = "dry-run")]
    pub dry_run: bool,
}

pub fn execute(args: RunArgs) -> anyhow::Result<()> {
    let initial_level = args.log_level.clone().unwrap_or_else(|| "info".to_string());
    crate::logging::init(&initial_level);

    let mut config = Config::load(&args.config)?;
    merge_cli(&mut config, &args)?;

    let result = crate::pipeline::run(&config, args.dry_run)?;
    tracing::info!(
        "完了: total={} accepted={} rejected={} errors={}",
        result.report.summary.total_input_records,
        result.report.summary.accepted_records,
        result.report.summary.rejected_records,
        result.report.summary.error_records,
    );
    Ok(())
}

fn merge_cli(config: &mut Config, args: &RunArgs) -> anyhow::Result<()> {
    if !args.input.is_empty() {
        config.input.paths = args.input.clone();
    }
    if let Some(input_format) = &args.input_format {
        config.input.format = match input_format.as_str() {
            "text" => InputFormat::Text,
            "jsonl" => InputFormat::Jsonl,
            other => anyhow::bail!("不正な --input-format です: {}", other),
        };
    }
    if let Some(output) = &args.output {
        config.output.path = output.clone();
    }
    if let Some(threads) = args.threads {
        config.runtime.threads = threads;
    }
    if let Some(batch_size) = args.batch_size {
        config.runtime.batch_size = batch_size;
    }
    if let Some(log_level) = &args.log_level {
        config.runtime.log_level = log_level.clone();
    }
    if let Some(stats_output) = &args.stats_output {
        config.stats.output_path = Some(stats_output.clone());
    }
    if let Some(stats_format) = &args.stats_format {
        config.stats.format = match stats_format.as_str() {
            "json" => StatsFormat::Json,
            "text" => StatsFormat::Text,
            other => anyhow::bail!("不正な --stats-format です: {}", other),
        };
    }
    config.validate()?;
    Ok(())
}
