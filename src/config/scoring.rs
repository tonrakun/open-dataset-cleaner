use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LanguageScoringConfig {
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub max_mixed_ratio: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct TextQualityScoringConfig {
    #[serde(default)]
    pub max_duplicate_line_ratio: Option<f64>,
    #[serde(default)]
    pub max_symbol_ratio: Option<f64>,
    #[serde(default)]
    pub perplexity_enabled: bool,
    /// KenLMのARPA/binary形式言語モデルファイルへのパス。
    /// `perplexity_enabled = true` の場合は必須(`perplexity` cargo featureが必要)。
    #[serde(default)]
    pub kenlm_model_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ContentQualityScoringConfig {
    /// 広告っぽいキーワードの完全一致リスト(大文字小文字を区別しない)。
    #[serde(default)]
    pub ad_keywords: Vec<String>,
    /// 広告/SEOスパム検出用の追加正規表現リスト(大文字小文字を区別しない)。
    #[serde(default)]
    pub ad_patterns: Vec<String>,
    #[serde(default)]
    pub max_ad_keyword_ratio: Option<f64>,
    #[serde(default)]
    pub max_seo_spam_score: Option<f64>,
    /// 文章の自然さスコア(0.0〜1.0、高いほど自然)の下限。
    /// PerplexityがOFFでも動作する簡易統計ベースのスコアで判定する。
    #[serde(default)]
    pub min_naturalness_score: Option<f64>,
}

impl ContentQualityScoringConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        for pattern in &self.ad_patterns {
            regex::Regex::new(pattern).map_err(|e| {
                anyhow::anyhow!("scoring.content_quality.ad_patterns の正規表現が不正です: {} ({})", pattern, e)
            })?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ScoringConfig {
    #[serde(default)]
    pub language: LanguageScoringConfig,
    #[serde(default)]
    pub text_quality: TextQualityScoringConfig,
    #[serde(default)]
    pub content_quality: ContentQualityScoringConfig,
}
