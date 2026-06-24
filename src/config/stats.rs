use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StatsFormat {
    #[default]
    Json,
    Text,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct StatsConfig {
    #[serde(default)]
    pub output_path: Option<String>,
    #[serde(default)]
    pub format: StatsFormat,
}
