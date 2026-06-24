use crate::config::{Config, InputFormat, OutputFormat};
use crate::dedup::Deduplicator;
use crate::extract::{Extractor, HtmlExtractor, PassthroughExtractor};
use crate::filter;
use crate::input::{discover_input_files, open_source};
use crate::output::{JsonlSink, ParquetSink, RecordSink};
use crate::plugin::PluginManager;
use crate::record::{RawRecord, RecordOutcome};
use crate::scoring::char_ratio::CharRatioScorer;
use crate::scoring::language::LanguageScorer;
use crate::scoring::text_quality::TextQualityScorer;
use crate::scoring::{run_all_scorers, Scorer};
use crate::stats::{StatsAccumulator, StatsReport};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub struct PipelineResult {
    pub report: StatsReport,
}

pub fn run(config: &Config, dry_run: bool) -> anyhow::Result<PipelineResult> {
    if config.runtime.threads > 0 {
        let _ = rayon::ThreadPoolBuilder::new()
            .num_threads(config.runtime.threads)
            .build_global();
    }

    let files = discover_input_files(&config.input.paths)?;
    if files.is_empty() {
        tracing::warn!("入力ファイルが見つかりませんでした");
    }

    let extractor: Box<dyn Extractor> = match config.input.format {
        InputFormat::Warc | InputFormat::Html => Box::new(HtmlExtractor::from_config(&config.extract)),
        InputFormat::Text | InputFormat::Jsonl => Box::new(PassthroughExtractor),
    };
    let scorers: Vec<Box<dyn Scorer>> = vec![
        Box::new(CharRatioScorer),
        Box::new(TextQualityScorer),
        Box::new(LanguageScorer::from_allowed_codes(&config.scoring.language.allow)?),
    ];
    let plugins = PluginManager::from_config(&config.plugins)?;

    let mut sink: Option<Box<dyn RecordSink>> = if dry_run {
        None
    } else {
        let rejected_path = if config.output.write_rejected {
            Some(PathBuf::from(config.output.rejected_path()))
        } else {
            None
        };
        Some(match config.output.format {
            OutputFormat::Jsonl => Box::new(JsonlSink::create(
                Path::new(&config.output.path),
                rejected_path.as_deref(),
            )?) as Box<dyn RecordSink>,
            OutputFormat::Parquet => Box::new(ParquetSink::create(
                Path::new(&config.output.path),
                rejected_path.as_deref(),
            )?) as Box<dyn RecordSink>,
        })
    };

    let mut dedup = Deduplicator::from_config(&config.dedup);
    let mut accumulator = StatsAccumulator::default();
    let start = Instant::now();
    let batch_size = config.runtime.batch_size.max(1);

    for file in &files {
        let mut source = match open_source(file, &config.input) {
            Ok(s) => s,
            Err(e) => {
                accumulator.record_outcome(&RecordOutcome::Error {
                    source_path: file.clone(),
                    message: e.to_string(),
                });
                continue;
            }
        };

        loop {
            let mut batch = Vec::with_capacity(batch_size);
            let mut reached_end = false;
            while batch.len() < batch_size {
                match source.next_record() {
                    Ok(Some(record)) => batch.push(record),
                    Ok(None) => {
                        reached_end = true;
                        break;
                    }
                    Err(e) => {
                        accumulator.record_outcome(&RecordOutcome::Error {
                            source_path: file.clone(),
                            message: e.to_string(),
                        });
                        reached_end = true;
                        break;
                    }
                }
            }

            if !batch.is_empty() {
                let outcomes: Vec<RecordOutcome> = batch
                    .into_par_iter()
                    .map(|raw| {
                        process_one(
                            raw,
                            extractor.as_ref(),
                            &scorers,
                            &config.scoring,
                            &config.filters,
                            plugins.as_ref(),
                        )
                    })
                    .collect();
                // 重複除去は採用済みレコード間の共有状態を更新するため、
                // スコアリングと異なり逐次処理する。
                let outcomes: Vec<RecordOutcome> =
                    outcomes.into_iter().map(|outcome| apply_dedup(outcome, &mut dedup)).collect();

                let batch_stats = outcomes
                    .par_iter()
                    .fold(StatsAccumulator::default, |mut acc, outcome| {
                        acc.record_outcome(outcome);
                        acc
                    })
                    .reduce(StatsAccumulator::default, |a, b| a.merge(b));
                accumulator = accumulator.merge(batch_stats);

                if let Some(sink) = sink.as_mut() {
                    for outcome in &outcomes {
                        match outcome {
                            RecordOutcome::Accepted { record, scores } => {
                                sink.write_accepted(record, scores)?;
                            }
                            RecordOutcome::Rejected { record, scores, reason } => {
                                sink.write_rejected(record, scores, reason)?;
                            }
                            RecordOutcome::Error { source_path, message } => {
                                tracing::warn!("レコード処理エラー {}: {}", source_path.display(), message);
                            }
                        }
                    }
                }
            }

            if reached_end {
                break;
            }
        }
    }

    if let Some(sink) = sink.as_mut() {
        sink.flush()?;
    }

    let report = accumulator.finalize(start.elapsed());
    let stats_path = config
        .stats
        .output_path
        .clone()
        .or_else(|| if dry_run { None } else { Some(format!("{}.stats.json", config.output.path)) });
    crate::stats::write_report(&report, stats_path.as_deref().map(Path::new), config.stats.format)?;

    Ok(PipelineResult { report })
}

fn apply_dedup(outcome: RecordOutcome, dedup: &mut Deduplicator) -> RecordOutcome {
    match outcome {
        RecordOutcome::Accepted { record, scores } => match dedup.check_and_insert(&record.text) {
            Some(reason) => RecordOutcome::Rejected { record, scores, reason },
            None => RecordOutcome::Accepted { record, scores },
        },
        other => other,
    }
}

fn process_one(
    raw: RawRecord,
    extractor: &dyn Extractor,
    scorers: &[Box<dyn Scorer>],
    scoring_cfg: &crate::config::ScoringConfig,
    filters_cfg: &crate::config::FiltersConfig,
    plugins: Option<&PluginManager>,
) -> RecordOutcome {
    let text = match extractor.extract(&raw) {
        Ok(t) => t,
        Err(e) => {
            return RecordOutcome::Error {
                source_path: raw.source_path.clone(),
                message: e.to_string(),
            }
        }
    };
    let mut scores = match run_all_scorers(&text, scorers) {
        Ok(s) => s,
        Err(e) => {
            return RecordOutcome::Error {
                source_path: raw.source_path.clone(),
                message: e.to_string(),
            }
        }
    };
    let mut record = raw;
    record.text = text;

    if let Some(plugins) = plugins {
        match plugins.evaluate(&record.text, &record.meta) {
            Ok(outcome) => {
                for (name, score) in outcome.scores {
                    scores.plugin_scores.insert(name, score);
                }
                if let Some(reason) = outcome.rejection {
                    return RecordOutcome::Rejected { record, scores, reason };
                }
            }
            Err(e) => {
                return RecordOutcome::Error {
                    source_path: record.source_path.clone(),
                    message: e.to_string(),
                }
            }
        }
    }

    match filter::evaluate(&scores, scoring_cfg, filters_cfg) {
        Ok(()) => RecordOutcome::Accepted { record, scores },
        Err(reason) => RecordOutcome::Rejected { record, scores, reason },
    }
}
