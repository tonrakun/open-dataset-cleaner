use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ThresholdConfig {
    #[serde(default)]
    pub reject_on_residual_html: bool,
    #[serde(default)]
    pub reject_on_residual_url: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FiltersConfig {
    #[serde(default)]
    pub thresholds: ThresholdConfig,
}
