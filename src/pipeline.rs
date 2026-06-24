use crate::checkpoint::{self, CheckpointState};
use crate::config::{Config, InputFormat, OutputFormat};
use crate::dedup::Deduplicator;
use crate::extract::{Extractor, HtmlExtractor, PassthroughExtractor};
use crate::filter;
use crate::input::{discover_input_files, open_source};
use crate::output::{JsonlSink, ParquetSink, RecordSink};
use crate::plugin::PluginManager;
use crate::record::{RawRecord, RecordOutcome};
use crate::scoring::char_ratio::CharRatioScorer;
use crate::scoring::content_quality::ContentQualityScorer;
use crate::scoring::language::LanguageScorer;
use crate::scoring::text_quality::TextQualityScorer;
use crate::scoring::{run_all_scorers, Scorer};
use crate::stats::{StatsAccumulator, StatsReport};
use rayon::prelude::*;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub struct PipelineResult {
    pub report: StatsReport,
}

/// ファイル読み込みスレッドからメイン処理スレッドへ渡す1バッチ分のメッセージ。
struct BatchMessage {
    batch: Vec<RawRecord>,
    error: Option<String>,
    reached_end: bool,
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
    #[cfg_attr(not(feature = "perplexity"), allow(unused_mut))]
    let mut scorers: Vec<Box<dyn Scorer>> = vec![
        Box::new(CharRatioScorer),
        Box::new(TextQualityScorer),
        Box::new(LanguageScorer::from_allowed_codes(&config.scoring.language.allow)?),
        Box::new(ContentQualityScorer::from_config(&config.scoring.content_quality)?),
    ];
    // perplexity_enabled時のfeature有効化・kenlm_model_path必須チェックはConfig::validateで
    // 既に行われているため、ここでは安全に組み立てるだけでよい。
    if config.scoring.text_quality.perplexity_enabled {
        #[cfg(feature = "perplexity")]
        {
            let model_path = config
                .scoring
                .text_quality
                .kenlm_model_path
                .as_deref()
                .expect("Config::validateでkenlm_model_path必須チェック済み");
            scorers.push(Box::new(crate::scoring::perplexity::PerplexityScorer::load(model_path)?));
        }
        #[cfg(not(feature = "perplexity"))]
        anyhow::bail!("perplexity featureが無効です(Config::validateで検出されるはずの状態です)");
    }
    let plugins = PluginManager::from_config(&config.plugins)?;

    // チェックポイント再開はJSONL出力のみ対応する。Parquetはファイルを閉じる(footerを
    // 書く)まで有効なファイルにならず、クラッシュ後に安全に追記再開できないため。
    let checkpoint_enabled =
        !dry_run && config.output.format == OutputFormat::Jsonl && config.runtime.checkpoint_dir.is_some();
    if config.runtime.checkpoint_dir.is_some() && !checkpoint_enabled {
        tracing::warn!(
            "runtime.checkpoint_dir はJSONL出力・非dry-run時のみ対応しているため、今回は無視して全件処理します"
        );
    }
    let checkpoint_dir = config
        .runtime
        .checkpoint_dir
        .as_ref()
        .filter(|_| checkpoint_enabled)
        .map(PathBuf::from);
    let fingerprint = checkpoint_dir.as_ref().map(|_| checkpoint::fingerprint(config));

    let mut dedup = Deduplicator::from_config(&config.dedup);
    let mut accumulator = StatsAccumulator::default();
    let mut completed_files: BTreeSet<String> = BTreeSet::new();

    if let (Some(dir), Some(fp)) = (&checkpoint_dir, &fingerprint) {
        if let Some(state) = checkpoint::load(dir)? {
            if &state.config_fingerprint == fp {
                dedup.restore(state.dedup);
                accumulator = StatsAccumulator::restore(state.stats);
                completed_files = state.completed_files;
                tracing::info!(
                    "チェックポイントから再開します(処理済み{}ファイルをスキップします)",
                    completed_files.len()
                );
            } else {
                tracing::warn!("設定が前回のチェックポイントと異なるため、最初から処理します");
            }
        }
    }

    let pending_files: Vec<PathBuf> = files
        .iter()
        .filter(|f| !completed_files.contains(&checkpoint::file_key(f)))
        .cloned()
        .collect();
    let skipped_count = files.len() - pending_files.len();
    if skipped_count > 0 {
        tracing::info!("チェックポイントにより{}件のファイルをスキップします", skipped_count);
    }
    let resuming_output = checkpoint_dir.is_some() && !completed_files.is_empty();

    let mut sink: Option<Box<dyn RecordSink>> = if dry_run {
        None
    } else {
        let rejected_path = if config.output.write_rejected {
            Some(PathBuf::from(config.output.rejected_path()))
        } else {
            None
        };
        Some(match config.output.format {
            OutputFormat::Jsonl => Box::new(JsonlSink::create_with_sharding(
                Path::new(&config.output.path),
                rejected_path.as_deref(),
                resuming_output,
                config.output.shard_max_rows,
            )?) as Box<dyn RecordSink>,
            OutputFormat::Parquet => Box::new(ParquetSink::create_with_sharding(
                Path::new(&config.output.path),
                rejected_path.as_deref(),
                config.output.shard_max_rows,
            )?) as Box<dyn RecordSink>,
        })
    };

    let start = Instant::now();
    let batch_size = config.runtime.batch_size.max(1);

    for file in &pending_files {
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

        // 次バッチの読み込み(I/O)をバックグラウンドスレッドで先行させ、
        // メインスレッドは現在のバッチのスコアリング・重複除去・書き込み(CPU)を行う。
        // ファイル単位の処理順序・バッチ内のレコード順序は変えないため、冪等性は保たれる。
        let (tx, rx) = std::sync::mpsc::sync_channel::<BatchMessage>(1);
        let reader_handle = std::thread::spawn(move || {
            loop {
                let mut batch = Vec::with_capacity(batch_size);
                let mut reached_end = false;
                let mut error = None;
                while batch.len() < batch_size {
                    match source.next_record() {
                        Ok(Some(record)) => batch.push(record),
                        Ok(None) => {
                            reached_end = true;
                            break;
                        }
                        Err(e) => {
                            error = Some(e.to_string());
                            reached_end = true;
                            break;
                        }
                    }
                }
                let is_last = reached_end;
                if tx.send(BatchMessage { batch, error, reached_end: is_last }).is_err() {
                    break;
                }
                if is_last {
                    break;
                }
            }
        });

        while let Ok(msg) = rx.recv() {
            if let Some(err) = msg.error {
                accumulator.record_outcome(&RecordOutcome::Error {
                    source_path: file.clone(),
                    message: err,
                });
            }

            if !msg.batch.is_empty() {
                let outcomes: Vec<RecordOutcome> = msg
                    .batch
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

            if msg.reached_end {
                break;
            }
        }
        reader_handle
            .join()
            .map_err(|_| anyhow::anyhow!("入力読み込みスレッドが異常終了しました: {}", file.display()))?;

        if let Some(dir) = &checkpoint_dir {
            completed_files.insert(checkpoint::file_key(file));
            let state = CheckpointState {
                config_fingerprint: fingerprint.clone().expect("checkpoint_dir設定時は必ずSome"),
                completed_files: completed_files.clone(),
                dedup: dedup.snapshot(),
                stats: accumulator.snapshot(),
            };
            checkpoint::save(dir, &state)?;
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
