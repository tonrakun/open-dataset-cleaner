mod extract;
mod filters;
mod input;
mod output;
mod runtime;
mod scoring;
mod stats;

pub use extract::ExtractConfig;
pub use filters::{FiltersConfig, ThresholdConfig};
pub use input::{InputConfig, InputFormat, PlainTextMode};
pub use output::OutputConfig;
pub use runtime::RuntimeConfig;
pub use scoring::{LanguageScoringConfig, ScoringConfig, TextQualityScoringConfig};
pub use stats::{StatsConfig, StatsFormat};

use std::path::Path;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Config {
    pub input: InputConfig,
    pub output: OutputConfig,
    #[serde(default)]
    pub extract: ExtractConfig,
    pub scoring: ScoringConfig,
    #[serde(default)]
    pub filters: FiltersConfig,
    #[serde(default)]
    pub runtime: RuntimeConfig,
    #[serde(default)]
    pub stats: StatsConfig,

    // 将来セクション(M2以降)。M1では受容するが効果なし=警告のみ。
    #[serde(default)]
    pub dedup: Option<toml::Value>,
    #[serde(default)]
    pub plugins: Option<toml::Value>,
}

impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("設定ファイル {} の読み込みに失敗: {}", path.display(), e))?;
        let config: Config = toml::from_str(&raw)
            .map_err(|e| anyhow::anyhow!("設定ファイル {} の解析に失敗: {}", path.display(), e))?;
        config.validate()?;
        if config.dedup.is_some() {
            tracing::warn!("[dedup] セクションはM1では未対応のため無視されます");
        }
        if config.plugins.is_some() {
            tracing::warn!("[[plugins]] セクションはM1では未対応のため無視されます");
        }
        Ok(config)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        match self.input.format {
            InputFormat::Text | InputFormat::Jsonl => {}
            InputFormat::Warc | InputFormat::Html => {
                anyhow::bail!(
                    "input.format = \"{:?}\" はM1では未実装です（text/jsonlのみ対応）",
                    self.input.format
                );
            }
        }
        if self.output.format != "jsonl" {
            anyhow::bail!("output.format = \"{}\" はM1では未実装です（jsonlのみ対応）", self.output.format);
        }
        if self.scoring.text_quality.perplexity_enabled {
            tracing::warn!("scoring.text_quality.perplexity_enabled はM1ではスタブのみのため無視されます");
        }
        if self.runtime.checkpoint_dir.is_some() {
            tracing::warn!("runtime.checkpoint_dir はM1では未対応のため無視されます");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_example_config() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("config.example.toml");
        let config = Config::load(&path).unwrap();
        assert_eq!(config.input.format, InputFormat::Jsonl);
        assert_eq!(config.input.paths, vec!["./data/**/*.jsonl".to_string()]);
        assert_eq!(config.output.path, "./out/dataset.jsonl");
        assert!(config.output.write_rejected);
        assert_eq!(config.scoring.language.allow, vec!["ja".to_string(), "en".to_string()]);
        assert_eq!(config.scoring.language.max_mixed_ratio, Some(0.2));
        assert!(config.filters.thresholds.reject_on_residual_html);
        assert_eq!(config.runtime.batch_size, 1000);
        assert_eq!(config.stats.format, StatsFormat::Json);
    }

    #[test]
    fn rejects_unsupported_input_format() {
        let toml_str = r#"
            [input]
            format = "warc"
            paths = []
            [output]
            path = "./out.jsonl"
            [scoring.language]
            allow = []
            [scoring.text_quality]
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.validate().is_err());
    }
}
