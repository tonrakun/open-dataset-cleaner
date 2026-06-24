use serde::Deserialize;

fn default_batch_size() -> usize {
    1000
}

fn default_log_level() -> String {
    "info".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeConfig {
    #[serde(default)]
    pub threads: usize,
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default)]
    pub checkpoint_dir: Option<String>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            threads: 0,
            batch_size: default_batch_size(),
            log_level: default_log_level(),
            checkpoint_dir: None,
        }
    }
}
