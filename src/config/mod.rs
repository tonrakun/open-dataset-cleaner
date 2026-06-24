mod dedup;
mod extract;
mod filters;
mod input;
mod output;
mod plugins;
mod runtime;
mod scoring;
mod stats;

pub use dedup::DedupConfig;
pub use extract::ExtractConfig;
pub use filters::{Condition, ConditionValue, FiltersConfig, NamedRule, Op, Rule, ThresholdConfig};
pub use input::{InputConfig, InputFormat, PlainTextMode};
pub use output::{OutputConfig, OutputFormat};
pub use plugins::PluginConfig;
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
    #[serde(default)]
    pub dedup: DedupConfig,
    #[serde(default)]
    pub plugins: Vec<PluginConfig>,
}

impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("設定ファイル {} の読み込みに失敗: {}", path.display(), e))?;
        let config: Config = toml::from_str(&raw)
            .map_err(|e| anyhow::anyhow!("設定ファイル {} の解析に失敗: {}", path.display(), e))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.scoring.text_quality.perplexity_enabled {
            if !cfg!(feature = "perplexity") {
                anyhow::bail!(
                    "scoring.text_quality.perplexity_enabled=true ですが `perplexity` cargo featureが無効です。\
                     `cargo build --features perplexity` (またはバイナリ配布版で同機能を有効にしたビルド)を使用してください。"
                );
            }
            if self.scoring.text_quality.kenlm_model_path.is_none() {
                anyhow::bail!(
                    "scoring.text_quality.perplexity_enabled=true の場合、kenlm_model_path の指定が必要です"
                );
            }
        }
        self.dedup.validate()?;
        self.filters.validate()?;
        for plugin in &self.plugins {
            plugin.validate()?;
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
        assert!(config.dedup.exact);
        assert!(config.dedup.minhash_lsh);
        assert_eq!(config.dedup.similarity_threshold, 0.85);
        assert_eq!(config.dedup.num_hashes, 128);
        assert_eq!(config.dedup.num_bands, 16);
    }

    #[test]
    fn rejects_dedup_num_hashes_not_divisible_by_num_bands() {
        let mut config = Config::default();
        config.dedup.minhash_lsh = true;
        config.dedup.num_hashes = 100;
        config.dedup.num_bands = 7;
        assert!(config.validate().is_err());
    }

    #[test]
    #[cfg(not(feature = "perplexity"))]
    fn rejects_perplexity_enabled_without_feature() {
        let mut config = Config::default();
        config.scoring.text_quality.perplexity_enabled = true;
        assert!(config.validate().is_err());
    }

    #[test]
    #[cfg(feature = "perplexity")]
    fn rejects_perplexity_enabled_without_model_path() {
        let mut config = Config::default();
        config.scoring.text_quality.perplexity_enabled = true;
        assert!(config.validate().is_err());

        config.scoring.text_quality.kenlm_model_path = Some("./model.bin".to_string());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn accepts_warc_and_html_input_formats() {
        let toml_str = r#"
            [input]
            format = "warc"
            paths = []
            [output]
            format = "parquet"
            path = "./out/dataset"
            [scoring.language]
            allow = []
            [scoring.text_quality]
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.input.format, InputFormat::Warc);
        assert_eq!(config.output.format, OutputFormat::Parquet);
        assert!(config.validate().is_ok());
    }
}
