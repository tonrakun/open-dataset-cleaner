use super::{ScoreSet, Scorer};
use once_cell::sync::Lazy;
use regex::Regex;
use unicode_segmentation::UnicodeSegmentation;

static HTML_TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"</?[a-zA-Z][a-zA-Z0-9]*[^>]*>").unwrap());
static URL_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"https?://[^\s]+").unwrap());

pub fn duplicate_line_ratio(text: &str) -> f64 {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return 0.0;
    }
    let mut seen = std::collections::HashSet::new();
    let mut duplicates = 0usize;
    for line in &lines {
        if !seen.insert(*line) {
            duplicates += 1;
        }
    }
    duplicates as f64 / lines.len() as f64
}

pub fn symbol_digit_ratio(text: &str) -> f64 {
    let total = text.chars().filter(|c| !c.is_whitespace()).count();
    if total == 0 {
        return 0.0;
    }
    let symbol_digit = text
        .chars()
        .filter(|c| !c.is_whitespace() && (c.is_ascii_digit() || (c.is_ascii_punctuation())))
        .count();
    symbol_digit as f64 / total as f64
}

pub fn sentence_length_stats(text: &str) -> (f64, f64) {
    let sentences: Vec<&str> = text
        .unicode_sentences()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if sentences.is_empty() {
        return (0.0, 0.0);
    }
    let lengths: Vec<f64> = sentences.iter().map(|s| s.chars().count() as f64).collect();
    let mean = lengths.iter().sum::<f64>() / lengths.len() as f64;
    if lengths.len() == 1 {
        return (mean, 0.0);
    }
    let variance = lengths.iter().map(|l| (l - mean).powi(2)).sum::<f64>() / lengths.len() as f64;
    (mean, variance)
}

pub fn has_residual_html_tags(text: &str) -> bool {
    HTML_TAG_RE.is_match(text)
}

pub fn has_residual_urls(text: &str) -> bool {
    URL_RE.is_match(text)
}

pub struct TextQualityScorer;

impl Scorer for TextQualityScorer {
    fn name(&self) -> &'static str {
        "text_quality"
    }

    fn score(&self, text: &str, scores: &mut ScoreSet) -> anyhow::Result<()> {
        scores.duplicate_line_ratio = duplicate_line_ratio(text);
        scores.symbol_digit_ratio = symbol_digit_ratio(text);
        let (avg, var) = sentence_length_stats(text);
        scores.avg_sentence_length = avg;
        scores.sentence_length_variance = var;
        scores.has_residual_html = has_residual_html_tags(text);
        scores.has_residual_url = has_residual_urls(text);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_line_ratio_counts_repeats() {
        let text = "a\nb\na\nc\na\n";
        // 重複行は2件("a"の2回目以降)、非空行5件
        assert!((duplicate_line_ratio(text) - 2.0 / 5.0).abs() < 1e-9);
    }

    #[test]
    fn duplicate_line_ratio_empty_is_zero() {
        assert_eq!(duplicate_line_ratio(""), 0.0);
    }

    #[test]
    fn symbol_digit_ratio_detects_digits_and_punctuation() {
        let text = "abc123!!!";
        // digits(3) + punctuation(3) = 6 / 9 non-whitespace chars
        assert!((symbol_digit_ratio(text) - 6.0 / 9.0).abs() < 1e-9);
    }

    #[test]
    fn sentence_stats_single_sentence() {
        let (avg, var) = sentence_length_stats("Hello world.");
        assert!(avg > 0.0);
        assert_eq!(var, 0.0);
    }

    #[test]
    fn detects_residual_html_and_url() {
        assert!(has_residual_html_tags("<div>text</div>"));
        assert!(!has_residual_html_tags("plain text"));
        assert!(has_residual_urls("see https://example.com for more"));
        assert!(!has_residual_urls("no links here"));
    }
}
