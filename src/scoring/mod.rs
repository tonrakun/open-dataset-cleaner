pub mod char_ratio;
pub mod content_quality;
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
    /// 広告っぽいキーワード/正規表現にマッチした語の比率(0.0〜1.0)。
    pub ad_keyword_ratio: f64,
    /// キーワード羅列・不自然な繰り返しパターンの検出スコア(0.0〜1.0、高いほどスパムらしい)。
    pub seo_spam_score: f64,
    /// 文章の自然さスコア(0.0〜1.0、高いほど自然)。Perplexityなしでも動く簡易統計版。
    pub naturalness_score: f64,
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
