//! 広告キーワード検出・SEOスパム検出・文章の自然さスコアを計算するスコアラー。
//!
//! 自然さスコアはKenLM等の言語モデルを使わない簡易統計ベースの近似であり、
//! `perplexity` featureの有無に関わらず常に動作する(7.4の軽量版要件)。

use super::{ScoreSet, Scorer};
use crate::config::ContentQualityScoringConfig;
use regex::Regex;
use std::collections::HashMap;
use unicode_segmentation::UnicodeSegmentation;

pub struct ContentQualityScorer {
    ad_patterns: Vec<Regex>,
}

impl ContentQualityScorer {
    pub fn from_config(cfg: &ContentQualityScoringConfig) -> anyhow::Result<Self> {
        let mut ad_patterns = Vec::with_capacity(cfg.ad_keywords.len() + cfg.ad_patterns.len());
        for keyword in &cfg.ad_keywords {
            ad_patterns.push(Regex::new(&format!("(?i){}", regex::escape(keyword)))?);
        }
        for pattern in &cfg.ad_patterns {
            ad_patterns.push(
                Regex::new(&format!("(?i){pattern}"))
                    .map_err(|e| anyhow::anyhow!("scoring.content_quality.ad_patterns が不正です: {}", e))?,
            );
        }
        Ok(Self { ad_patterns })
    }

    fn ad_keyword_ratio(&self, text: &str, word_count: usize) -> f64 {
        if self.ad_patterns.is_empty() || word_count == 0 {
            return 0.0;
        }
        let matches: usize = self.ad_patterns.iter().map(|re| re.find_iter(text).count()).sum();
        (matches as f64 / word_count as f64).min(1.0)
    }
}

/// 同一語の連続repeatや、語の出現頻度の偏りからキーワード羅列(SEOスパム)らしさを推定する。
fn seo_spam_score(words: &[String]) -> f64 {
    if words.len() < 4 {
        return 0.0;
    }
    let mut freq: HashMap<&str, usize> = HashMap::new();
    for w in words {
        *freq.entry(w.as_str()).or_insert(0) += 1;
    }
    let top_freq_ratio = freq.values().copied().max().unwrap_or(0) as f64 / words.len() as f64;

    let mut max_repeat_run = 1usize;
    let mut current_run = 1usize;
    for i in 1..words.len() {
        if words[i] == words[i - 1] {
            current_run += 1;
            max_repeat_run = max_repeat_run.max(current_run);
        } else {
            current_run = 1;
        }
    }
    let repeat_run_ratio = (max_repeat_run as f64 / words.len() as f64).min(1.0);

    top_freq_ratio.max(repeat_run_ratio)
}

/// 同じ文字の3連続以上の繰り返しのみで構成される語(例: "ーーー")を文字化けやキーボード
/// スパムの代理指標として扱う。
fn is_repeated_char_run(word: &str) -> bool {
    let chars: Vec<char> = word.chars().collect();
    chars.len() >= 3 && chars.iter().all(|c| *c == chars[0])
}

/// KenLM等の言語モデルを使わない自然さスコアの近似値。平均語長・反復語(文字化け)比率・
/// SEOスパムスコアの3指標を減点方式で合成する(0.0〜1.0、高いほど自然)。
fn naturalness_score(words: &[String], spam_score: f64) -> f64 {
    if words.is_empty() {
        return 0.0;
    }
    let total = words.len() as f64;
    let avg_word_len = words.iter().map(|w| w.chars().count()).sum::<usize>() as f64 / total;
    let length_penalty = if avg_word_len > 12.0 { ((avg_word_len - 12.0) / 12.0).min(1.0) } else { 0.0 };
    let gibberish_count = words.iter().filter(|w| is_repeated_char_run(w)).count();
    let gibberish_penalty = gibberish_count as f64 / total;
    (1.0 - length_penalty - gibberish_penalty - spam_score * 0.5).clamp(0.0, 1.0)
}

impl Scorer for ContentQualityScorer {
    fn name(&self) -> &'static str {
        "content_quality"
    }

    fn score(&self, text: &str, scores: &mut ScoreSet) -> anyhow::Result<()> {
        let words: Vec<String> = text.unicode_words().map(|w| w.to_lowercase()).collect();
        scores.ad_keyword_ratio = self.ad_keyword_ratio(text, words.len());
        scores.seo_spam_score = seo_spam_score(&words);
        scores.naturalness_score = naturalness_score(&words, scores.seo_spam_score);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scorer(cfg: ContentQualityScoringConfig) -> ContentQualityScorer {
        ContentQualityScorer::from_config(&cfg).unwrap()
    }

    #[test]
    fn ad_keyword_ratio_is_zero_without_keywords() {
        let s = scorer(ContentQualityScoringConfig::default());
        let mut scores = ScoreSet::default();
        s.score("今すぐ購入！激安セール！", &mut scores).unwrap();
        assert_eq!(scores.ad_keyword_ratio, 0.0);
    }

    #[test]
    fn ad_keyword_ratio_detects_configured_keywords() {
        let cfg = ContentQualityScoringConfig {
            ad_keywords: vec!["sale".to_string(), "buy now".to_string()],
            ..Default::default()
        };
        let s = scorer(cfg);
        let mut scores = ScoreSet::default();
        s.score("Big SALE today, buy now and save big!", &mut scores).unwrap();
        assert!(scores.ad_keyword_ratio > 0.0);
    }

    #[test]
    fn ad_keyword_ratio_supports_regex_patterns() {
        let cfg = ContentQualityScoringConfig { ad_patterns: vec![r"\d+%\s*off".to_string()], ..Default::default() };
        let s = scorer(cfg);
        let mut scores = ScoreSet::default();
        s.score("today only: 50% off everything", &mut scores).unwrap();
        assert!(scores.ad_keyword_ratio > 0.0);
    }

    #[test]
    fn seo_spam_score_detects_keyword_stuffing() {
        let s = scorer(ContentQualityScoringConfig::default());
        let mut scores = ScoreSet::default();
        s.score("buy buy buy buy buy now now now now", &mut scores).unwrap();
        assert!(scores.seo_spam_score > 0.5);
    }

    #[test]
    fn seo_spam_score_is_low_for_natural_text() {
        let s = scorer(ContentQualityScoringConfig::default());
        let mut scores = ScoreSet::default();
        s.score("The quick brown fox jumps over the lazy dog near the river bank.", &mut scores).unwrap();
        assert!(scores.seo_spam_score < 0.3);
    }

    #[test]
    fn naturalness_score_is_high_for_normal_prose() {
        let s = scorer(ContentQualityScoringConfig::default());
        let mut scores = ScoreSet::default();
        s.score("This is a perfectly ordinary sentence about everyday life.", &mut scores).unwrap();
        assert!(scores.naturalness_score > 0.7);
    }

    #[test]
    fn naturalness_score_is_low_for_repeated_gibberish() {
        let s = scorer(ContentQualityScoringConfig::default());
        let mut scores = ScoreSet::default();
        s.score("aaaa bbbb aaaa bbbb aaaa bbbb aaaa bbbb", &mut scores).unwrap();
        assert!(scores.naturalness_score < 0.5);
    }

    #[test]
    fn empty_text_yields_zero_scores() {
        let s = scorer(ContentQualityScoringConfig::default());
        let mut scores = ScoreSet::default();
        s.score("", &mut scores).unwrap();
        assert_eq!(scores.ad_keyword_ratio, 0.0);
        assert_eq!(scores.seo_spam_score, 0.0);
        assert_eq!(scores.naturalness_score, 0.0);
    }

    #[test]
    fn rejects_invalid_ad_pattern_regex() {
        let cfg = ContentQualityScoringConfig { ad_patterns: vec!["(unterminated".to_string()], ..Default::default() };
        assert!(ContentQualityScorer::from_config(&cfg).is_err());
    }
}
