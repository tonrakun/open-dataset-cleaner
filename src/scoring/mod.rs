pub mod char_ratio;
pub mod language;
#[cfg(feature = "perplexity")]
pub mod perplexity;
pub mod text_quality;

use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
pub struct CharRatios {
    pub hiragana: f64,
    pub katakana: f64,
    pub kanji: f64,
    pub alnum: f64,
    pub other: f64,
}

/// 文字種・記号判定上の「その他」には空白・句読点・改行を含む。
#[derive(Debug, Clone, Default, Serialize)]
pub struct ScoreSet {
    pub detected_language: Option<String>,
    pub language_mixed_ratio: f64,
    pub char_ratios: CharRatios,
    pub duplicate_line_ratio: f64,
    pub symbol_digit_ratio: f64,
    pub avg_sentence_length: f64,
    pub sentence_length_variance: f64,
    pub has_residual_html: bool,
    pub has_residual_url: bool,
    /// KenLM言語モデルによるperplexityスコア。`perplexity` feature無効時は常にNone。
    #[cfg(feature = "perplexity")]
    pub perplexity: Option<f64>,
    /// WASMプラグインが返したスコア。キーはプラグイン名。
    pub plugin_scores: std::collections::BTreeMap<String, f64>,
}

pub trait Scorer: Send + Sync {
    fn name(&self) -> &'static str;
    fn score(&self, text: &str, scores: &mut ScoreSet) -> anyhow::Result<()>;
}

pub fn run_all_scorers(text: &str, scorers: &[Box<dyn Scorer>]) -> anyhow::Result<ScoreSet> {
    let mut scores = ScoreSet::default();
    for scorer in scorers {
        scorer
            .score(text, &mut scores)
            .map_err(|e| anyhow::anyhow!("scorer '{}' failed: {}", scorer.name(), e))?;
    }
    Ok(scores)
}
