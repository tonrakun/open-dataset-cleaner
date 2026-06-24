use serde::Deserialize;

/// WASMプラグインの設定。`path` はローカルファイルパス(.wasm/.wat)。
#[derive(Debug, Clone, Deserialize)]
pub struct PluginConfig {
    pub name: String,
    pub path: String,
    #[serde(default = "default_plugin_table")]
    pub config: toml::Value,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_memory_limit_bytes")]
    pub memory_limit_bytes: usize,
}

fn default_plugin_table() -> toml::Value {
    toml::Value::Table(toml::map::Map::new())
}

fn default_timeout_ms() -> u64 {
    100
}

fn default_memory_limit_bytes() -> usize {
    16 * 1024 * 1024
}

impl PluginConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.name.trim().is_empty() {
            anyhow::bail!("plugins[].name は空にできません");
        }
        if self.path.trim().is_empty() {
            anyhow::bail!("plugins[].path は空にできません (name={})", self.name);
        }
        if self.timeout_ms == 0 {
            anyhow::bail!("plugins[].timeout_ms は1以上を指定してください (name={})", self.name);
        }
        if self.memory_limit_bytes == 0 {
            anyhow::bail!("plugins[].memory_limit_bytes は1以上を指定してください (name={})", self.name);
        }
        Ok(())
    }

    /// プラグインに渡す設定値をJSONへ変換する。
    pub fn config_as_json(&self) -> serde_json::Value {
        toml_to_json(&self.config)
    }
}

fn toml_to_json(value: &toml::Value) -> serde_json::Value {
    match value {
        toml::Value::String(s) => serde_json::Value::String(s.clone()),
        toml::Value::Integer(i) => serde_json::Value::Number((*i).into()),
        toml::Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        toml::Value::Boolean(b) => serde_json::Value::Bool(*b),
        toml::Value::Datetime(d) => serde_json::Value::String(d.to_string()),
        toml::Value::Array(arr) => serde_json::Value::Array(arr.iter().map(toml_to_json).collect()),
        toml::Value::Table(table) => {
            serde_json::Value::Object(table.iter().map(|(k, v)| (k.clone(), toml_to_json(v))).collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_name() {
        let config = PluginConfig {
            name: "".to_string(),
            path: "./plugin.wasm".to_string(),
            config: default_plugin_table(),
            timeout_ms: 100,
            memory_limit_bytes: 1024,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_zero_timeout() {
        let config = PluginConfig {
            name: "p".to_string(),
            path: "./plugin.wasm".to_string(),
            config: default_plugin_table(),
            timeout_ms: 0,
            memory_limit_bytes: 1024,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn converts_toml_table_to_json() {
        let toml_str = r#"sensitivity = "medium"
threshold = 0.5
enabled = true
tags = ["a", "b"]
"#;
        let value: toml::Value = toml::from_str(toml_str).unwrap();
        let config = PluginConfig {
            name: "p".to_string(),
            path: "./plugin.wasm".to_string(),
            config: value,
            timeout_ms: 100,
            memory_limit_bytes: 1024,
        };
        let json = config.config_as_json();
        assert_eq!(json["sensitivity"], "medium");
        assert_eq!(json["threshold"], 0.5);
        assert_eq!(json["enabled"], true);
        assert_eq!(json["tags"][0], "a");
    }
}
