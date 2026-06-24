pub mod histogram;

pub use histogram::{RunningStats, RunningStatsSnapshot, RunningStatsSummary};

use crate::config::StatsFormat;
use crate::record::RecordOutcome;
use crate::scoring::ScoreSet;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct StatsAccumulator {
    pub total: u64,
    pub accepted: u64,
    pub rejected: u64,
    pub errors: u64,
    pub rejection_reasons: BTreeMap<String, u64>,
    pub language_mixed_ratio: RunningStats,
    pub duplicate_line_ratio: RunningStats,
    pub symbol_digit_ratio: RunningStats,
    pub avg_sentence_length: RunningStats,
    pub sentence_length_variance: RunningStats,
    pub hiragana_ratio: RunningStats,
    pub katakana_ratio: RunningStats,
    pub kanji_ratio: RunningStats,
    pub alnum_ratio: RunningStats,
    pub other_ratio: RunningStats,
    pub ad_keyword_ratio: RunningStats,
    pub seo_spam_score: RunningStats,
    pub naturalness_score: RunningStats,
}

impl StatsAccumulator {
    pub fn record_outcome(&mut self, outcome: &RecordOutcome) {
        self.total += 1;
        match outcome {
            RecordOutcome::Accepted { scores, .. } => {
                self.accepted += 1;
                self.push_scores(scores);
            }
            RecordOutcome::Rejected { scores, reason, .. } => {
                self.rejected += 1;
                self.push_scores(scores);
                *self.rejection_reasons.entry(reason.as_key()).or_insert(0) += 1;
            }
            RecordOutcome::Error { .. } => {
                self.errors += 1;
            }
        }
    }

    fn push_scores(&mut self, scores: &ScoreSet) {
        self.language_mixed_ratio.push(scores.language_mixed_ratio);
        self.duplicate_line_ratio.push(scores.duplicate_line_ratio);
        self.symbol_digit_ratio.push(scores.symbol_digit_ratio);
        self.avg_sentence_length.push(scores.avg_sentence_length);
        self.sentence_length_variance.push(scores.sentence_length_variance);
        self.hiragana_ratio.push(scores.char_ratios.hiragana);
        self.katakana_ratio.push(scores.char_ratios.katakana);
        self.kanji_ratio.push(scores.char_ratios.kanji);
        self.alnum_ratio.push(scores.char_ratios.alnum);
        self.other_ratio.push(scores.char_ratios.other);
        self.ad_keyword_ratio.push(scores.ad_keyword_ratio);
        self.seo_spam_score.push(scores.seo_spam_score);
        self.naturalness_score.push(scores.naturalness_score);
    }

    pub fn merge(self, other: Self) -> Self {
        let mut rejection_reasons = self.rejection_reasons;
        for (k, v) in other.rejection_reasons {
            *rejection_reasons.entry(k).or_insert(0) += v;
        }
        Self {
            total: self.total + other.total,
            accepted: self.accepted + other.accepted,
            rejected: self.rejected + other.rejected,
            errors: self.errors + other.errors,
            rejection_reasons,
            language_mixed_ratio: self.language_mixed_ratio.merge(&other.language_mixed_ratio),
            duplicate_line_ratio: self.duplicate_line_ratio.merge(&other.duplicate_line_ratio),
            symbol_digit_ratio: self.symbol_digit_ratio.merge(&other.symbol_digit_ratio),
            avg_sentence_length: self.avg_sentence_length.merge(&other.avg_sentence_length),
            sentence_length_variance: self.sentence_length_variance.merge(&other.sentence_length_variance),
            hiragana_ratio: self.hiragana_ratio.merge(&other.hiragana_ratio),
            katakana_ratio: self.katakana_ratio.merge(&other.katakana_ratio),
            kanji_ratio: self.kanji_ratio.merge(&other.kanji_ratio),
            alnum_ratio: self.alnum_ratio.merge(&other.alnum_ratio),
            other_ratio: self.other_ratio.merge(&other.other_ratio),
            ad_keyword_ratio: self.ad_keyword_ratio.merge(&other.ad_keyword_ratio),
            seo_spam_score: self.seo_spam_score.merge(&other.seo_spam_score),
            naturalness_score: self.naturalness_score.merge(&other.naturalness_score),
        }
    }

    /// チェックポイント保存用のスナップショットに変換する。
    pub fn snapshot(&self) -> StatsAccumulatorSnapshot {
        StatsAccumulatorSnapshot {
            total: self.total,
            accepted: self.accepted,
            rejected: self.rejected,
            errors: self.errors,
            rejection_reasons: self.rejection_reasons.clone(),
            language_mixed_ratio: self.language_mixed_ratio.snapshot(),
            duplicate_line_ratio: self.duplicate_line_ratio.snapshot(),
            symbol_digit_ratio: self.symbol_digit_ratio.snapshot(),
            avg_sentence_length: self.avg_sentence_length.snapshot(),
            sentence_length_variance: self.sentence_length_variance.snapshot(),
            hiragana_ratio: self.hiragana_ratio.snapshot(),
            katakana_ratio: self.katakana_ratio.snapshot(),
            kanji_ratio: self.kanji_ratio.snapshot(),
            alnum_ratio: self.alnum_ratio.snapshot(),
            other_ratio: self.other_ratio.snapshot(),
            ad_keyword_ratio: self.ad_keyword_ratio.snapshot(),
            seo_spam_score: self.seo_spam_score.snapshot(),
            naturalness_score: self.naturalness_score.snapshot(),
        }
    }

    pub fn restore(snapshot: StatsAccumulatorSnapshot) -> Self {
        Self {
            total: snapshot.total,
            accepted: snapshot.accepted,
            rejected: snapshot.rejected,
            errors: snapshot.errors,
            rejection_reasons: snapshot.rejection_reasons,
            language_mixed_ratio: RunningStats::restore(snapshot.language_mixed_ratio),
            duplicate_line_ratio: RunningStats::restore(snapshot.duplicate_line_ratio),
            symbol_digit_ratio: RunningStats::restore(snapshot.symbol_digit_ratio),
            avg_sentence_length: RunningStats::restore(snapshot.avg_sentence_length),
            sentence_length_variance: RunningStats::restore(snapshot.sentence_length_variance),
            hiragana_ratio: RunningStats::restore(snapshot.hiragana_ratio),
            katakana_ratio: RunningStats::restore(snapshot.katakana_ratio),
            kanji_ratio: RunningStats::restore(snapshot.kanji_ratio),
            alnum_ratio: RunningStats::restore(snapshot.alnum_ratio),
            other_ratio: RunningStats::restore(snapshot.other_ratio),
            ad_keyword_ratio: RunningStats::restore(snapshot.ad_keyword_ratio),
            seo_spam_score: RunningStats::restore(snapshot.seo_spam_score),
            naturalness_score: RunningStats::restore(snapshot.naturalness_score),
        }
    }

    pub fn finalize(&self, elapsed: Duration) -> StatsReport {
        StatsReport {
            summary: Summary {
                total_input_records: self.total,
                accepted_records: self.accepted,
                rejected_records: self.rejected,
                error_records: self.errors,
                elapsed_seconds: elapsed.as_secs_f64(),
            },
            rejection_reasons: self.rejection_reasons.clone(),
            score_distributions: ScoreDistributions {
                language_mixed_ratio: self.language_mixed_ratio.summary(),
                duplicate_line_ratio: self.duplicate_line_ratio.summary(),
                symbol_digit_ratio: self.symbol_digit_ratio.summary(),
                avg_sentence_length: self.avg_sentence_length.summary(),
                sentence_length_variance: self.sentence_length_variance.summary(),
            },
            char_type_ratios_aggregate: CharTypeRatiosAggregate {
                hiragana: self.hiragana_ratio.summary().mean,
                katakana: self.katakana_ratio.summary().mean,
                kanji: self.kanji_ratio.summary().mean,
                alnum: self.alnum_ratio.summary().mean,
                other: self.other_ratio.summary().mean,
            },
            content_quality: ContentQualityDistributions {
                ad_keyword_ratio: self.ad_keyword_ratio.summary(),
                seo_spam_score: self.seo_spam_score.summary(),
                naturalness_score: self.naturalness_score.summary(),
            },
        }
    }
}

/// チェックポイント保存用のシリアライズ可能なスナップショット。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsAccumulatorSnapshot {
    pub total: u64,
    pub accepted: u64,
    pub rejected: u64,
    pub errors: u64,
    pub rejection_reasons: BTreeMap<String, u64>,
    pub language_mixed_ratio: RunningStatsSnapshot,
    pub duplicate_line_ratio: RunningStatsSnapshot,
    pub symbol_digit_ratio: RunningStatsSnapshot,
    pub avg_sentence_length: RunningStatsSnapshot,
    pub sentence_length_variance: RunningStatsSnapshot,
    pub hiragana_ratio: RunningStatsSnapshot,
    pub katakana_ratio: RunningStatsSnapshot,
    pub kanji_ratio: RunningStatsSnapshot,
    pub alnum_ratio: RunningStatsSnapshot,
    pub other_ratio: RunningStatsSnapshot,
    #[serde(default)]
    pub ad_keyword_ratio: RunningStatsSnapshot,
    #[serde(default)]
    pub seo_spam_score: RunningStatsSnapshot,
    #[serde(default)]
    pub naturalness_score: RunningStatsSnapshot,
}

#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub total_input_records: u64,
    pub accepted_records: u64,
    pub rejected_records: u64,
    pub error_records: u64,
    pub elapsed_seconds: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScoreDistributions {
    pub language_mixed_ratio: RunningStatsSummary,
    pub duplicate_line_ratio: RunningStatsSummary,
    pub symbol_digit_ratio: RunningStatsSummary,
    pub avg_sentence_length: RunningStatsSummary,
    pub sentence_length_variance: RunningStatsSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct CharTypeRatiosAggregate {
    pub hiragana: f64,
    pub katakana: f64,
    pub kanji: f64,
    pub alnum: f64,
    pub other: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContentQualityDistributions {
    pub ad_keyword_ratio: RunningStatsSummary,
    pub seo_spam_score: RunningStatsSummary,
    pub naturalness_score: RunningStatsSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatsReport {
    pub summary: Summary,
    pub rejection_reasons: BTreeMap<String, u64>,
    pub score_distributions: ScoreDistributions,
    pub char_type_ratios_aggregate: CharTypeRatiosAggregate,
    pub content_quality: ContentQualityDistributions,
}

pub fn write_report(report: &StatsReport, path: Option<&Path>, format: StatsFormat) -> anyhow::Result<()> {
    let rendered = match format {
        StatsFormat::Json => serde_json::to_string_pretty(report)?,
        StatsFormat::Text => render_text(report),
    };
    match path {
        Some(p) => {
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(p, rendered)?;
        }
        None => println!("{}", rendered),
    }
    Ok(())
}

fn render_text(report: &StatsReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "total={} accepted={} rejected={} errors={} elapsed={:.2}s\n",
        report.summary.total_input_records,
        report.summary.accepted_records,
        report.summary.rejected_records,
        report.summary.error_records,
        report.summary.elapsed_seconds,
    ));
    out.push_str("rejection_reasons:\n");
    for (reason, count) in &report.rejection_reasons {
        out.push_str(&format!("  {}: {}\n", reason, count));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::{RawRecord, RejectionReason};

    fn dummy_record() -> RawRecord {
        RawRecord {
            id: "id".to_string(),
            source_path: "f".into(),
            text: "t".to_string(),
            meta: Default::default(),
        }
    }

    #[test]
    fn record_outcome_tracks_counts_and_reasons() {
        let mut acc = StatsAccumulator::default();
        acc.record_outcome(&RecordOutcome::Accepted { record: dummy_record(), scores: ScoreSet::default() });
        acc.record_outcome(&RecordOutcome::Rejected {
            record: dummy_record(),
            scores: ScoreSet::default(),
            reason: RejectionReason::LanguageNotAllowed,
        });
        acc.record_outcome(&RecordOutcome::Error { source_path: "f".into(), message: "x".to_string() });

        assert_eq!(acc.total, 3);
        assert_eq!(acc.accepted, 1);
        assert_eq!(acc.rejected, 1);
        assert_eq!(acc.errors, 1);
        assert_eq!(acc.rejection_reasons.get("language_not_allowed"), Some(&1));
    }

    #[test]
    fn snapshot_round_trip_via_json_preserves_counts_and_distributions() {
        let mut acc = StatsAccumulator::default();
        acc.record_outcome(&RecordOutcome::Accepted { record: dummy_record(), scores: ScoreSet::default() });
        acc.record_outcome(&RecordOutcome::Rejected {
            record: dummy_record(),
            scores: ScoreSet::default(),
            reason: RejectionReason::LanguageNotAllowed,
        });

        let json = serde_json::to_string(&acc.snapshot()).unwrap();
        let restored = StatsAccumulator::restore(serde_json::from_str(&json).unwrap());

        assert_eq!(restored.total, 2);
        assert_eq!(restored.accepted, 1);
        assert_eq!(restored.rejected, 1);
        assert_eq!(restored.rejection_reasons.get("language_not_allowed"), Some(&1));
        assert_eq!(restored.duplicate_line_ratio.count, acc.duplicate_line_ratio.count);
    }

    #[test]
    fn empty_accumulator_snapshot_round_trips_via_json() {
        let acc = StatsAccumulator::default();
        let json = serde_json::to_string(&acc.snapshot()).unwrap();
        let restored = StatsAccumulator::restore(serde_json::from_str(&json).unwrap());
        assert_eq!(restored.total, 0);
        assert_eq!(restored.duplicate_line_ratio.summary().min, 0.0);
    }

    #[test]
    fn merge_combines_two_accumulators() {
        let mut a = StatsAccumulator::default();
        a.record_outcome(&RecordOutcome::Accepted { record: dummy_record(), scores: ScoreSet::default() });
        let mut b = StatsAccumulator::default();
        b.record_outcome(&RecordOutcome::Accepted { record: dummy_record(), scores: ScoreSet::default() });
        let merged = a.merge(b);
        assert_eq!(merged.total, 2);
        assert_eq!(merged.accepted, 2);
    }
}
