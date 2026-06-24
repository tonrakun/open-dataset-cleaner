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
pub struct ScoringConfig {
    #[serde(default)]
    pub language: LanguageScoringConfig,
    #[serde(default)]
    pub text_quality: TextQualityScoringConfig,
}
