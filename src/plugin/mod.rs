mod wasm;

use crate::config::PluginConfig;
use crate::record::RejectionReason;
use serde_json::{Map, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use wasmtime::{Config as WasmConfig, Engine};
use wasm::WasmPlugin;

const EPOCH_TICK: Duration = Duration::from_millis(5);

/// 全プラグインを1レコードに対して評価した結果。
pub struct PluginOutcome {
    /// `(プラグイン名, スコア)`。スコアを返したプラグインのみ含む。
    pub scores: Vec<(String, f64)>,
    /// いずれかのプラグインが採用拒否(`accept: false`)した場合の除外理由。
    pub rejection: Option<RejectionReason>,
}

/// 設定済みのWASMプラグイン群をロードし、サンドボックス実行する。
pub struct PluginManager {
    plugins: Vec<WasmPlugin>,
    stop: Arc<AtomicBool>,
}

impl PluginManager {
    /// `configs`が空であれば`None`を返す(プラグイン未使用時はwasmtimeエンジンを起動しない)。
    pub fn from_config(configs: &[PluginConfig]) -> anyhow::Result<Option<Self>> {
        if configs.is_empty() {
            return Ok(None);
        }

        let mut wasm_config = WasmConfig::new();
        wasm_config.epoch_interruption(true);
        let engine = Engine::new(&wasm_config)?;

        let plugins = configs
            .iter()
            .map(|c| WasmPlugin::load(&engine, c, EPOCH_TICK))
            .collect::<anyhow::Result<Vec<_>>>()?;

        // epoch_interruptionによるタイムアウト判定を有効化するため、
        // 一定間隔でエンジンのepochを進める専用スレッドを起動する。
        let stop = Arc::new(AtomicBool::new(false));
        let ticker_engine = engine.clone();
        let ticker_stop = stop.clone();
        thread::spawn(move || {
            while !ticker_stop.load(Ordering::Relaxed) {
                thread::sleep(EPOCH_TICK);
                ticker_engine.increment_epoch();
            }
        });

        Ok(Some(Self { plugins, stop }))
    }

    /// 全プラグインを順に評価する。`accept: false`を返したプラグインがあれば
    /// 以降のプラグインは評価せず即座に除外理由を返す。
    pub fn evaluate(&self, text: &str, meta: &Map<String, Value>) -> anyhow::Result<PluginOutcome> {
        let mut scores = Vec::new();
        for plugin in &self.plugins {
            let result = plugin
                .call(text, meta)
                .map_err(|e| anyhow::anyhow!("プラグイン {} の実行に失敗しました: {}", plugin.name(), e))?;
            if let Some(score) = result.score {
                scores.push((plugin.name().to_string(), score));
            }
            if result.accept == Some(false) {
                let detail = result.reason.unwrap_or_else(|| "rejected".to_string());
                return Ok(PluginOutcome {
                    scores,
                    rejection: Some(RejectionReason::Plugin(format!("{}:{}", plugin.name(), detail))),
                });
            }
        }
        Ok(PluginOutcome { scores, rejection: None })
    }
}

impl Drop for PluginManager {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fixture(name: &str) -> String {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/plugins")
            .join(name)
            .to_string_lossy()
            .into_owned()
    }

    fn plugin_config(name: &str, file: &str, timeout_ms: u64, memory_limit_bytes: usize) -> PluginConfig {
        PluginConfig {
            name: name.to_string(),
            path: fixture(file),
            config: toml::Value::Table(toml::map::Map::new()),
            timeout_ms,
            memory_limit_bytes,
        }
    }

    #[test]
    fn empty_config_returns_no_manager() {
        assert!(PluginManager::from_config(&[]).unwrap().is_none());
    }

    #[test]
    fn accept_plugin_returns_score_without_rejection() {
        let configs = vec![plugin_config("accept", "accept.wat", 200, 16 * 1024 * 1024)];
        let manager = PluginManager::from_config(&configs).unwrap().unwrap();
        let outcome = manager.evaluate("hello", &Map::new()).unwrap();
        assert!(outcome.rejection.is_none());
        assert_eq!(outcome.scores, vec![("accept".to_string(), 0.9)]);
    }

    #[test]
    fn reject_plugin_short_circuits_with_reason() {
        let configs = vec![plugin_config("reject", "reject.wat", 200, 16 * 1024 * 1024)];
        let manager = PluginManager::from_config(&configs).unwrap().unwrap();
        let outcome = manager.evaluate("hello", &Map::new()).unwrap();
        match outcome.rejection {
            Some(RejectionReason::Plugin(detail)) => assert_eq!(detail, "reject:blocked"),
            other => panic!("expected plugin rejection, got {:?}", other),
        }
    }

    #[test]
    fn all_plugin_scores_are_recorded_even_when_a_later_plugin_rejects() {
        let configs = vec![
            plugin_config("accept", "accept.wat", 200, 16 * 1024 * 1024),
            plugin_config("reject", "reject.wat", 200, 16 * 1024 * 1024),
        ];
        let manager = PluginManager::from_config(&configs).unwrap().unwrap();
        let outcome = manager.evaluate("hello", &Map::new()).unwrap();
        assert_eq!(outcome.scores, vec![("accept".to_string(), 0.9), ("reject".to_string(), 0.1)]);
        assert!(outcome.rejection.is_some());
    }

    #[test]
    fn malformed_plugin_output_is_an_error() {
        let configs = vec![plugin_config("bad", "bad_output.wat", 200, 16 * 1024 * 1024)];
        let manager = PluginManager::from_config(&configs).unwrap().unwrap();
        assert!(manager.evaluate("hello", &Map::new()).is_err());
    }

    #[test]
    fn runaway_plugin_is_interrupted_by_timeout() {
        let configs = vec![plugin_config("timeout", "timeout.wat", 20, 16 * 1024 * 1024)];
        let manager = PluginManager::from_config(&configs).unwrap().unwrap();
        assert!(manager.evaluate("hello", &Map::new()).is_err());
    }

    #[test]
    fn oversized_plugin_memory_is_rejected_by_limiter() {
        let configs = vec![plugin_config("hog", "memory_hog.wat", 200, 1024 * 1024)];
        let manager = PluginManager::from_config(&configs).unwrap().unwrap();
        assert!(manager.evaluate("hello", &Map::new()).is_err());
    }
}
