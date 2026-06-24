use serde::Deserialize;

fn default_format() -> String {
    "jsonl".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct OutputConfig {
    #[serde(default = "default_format")]
    pub format: String,
    pub path: String,
    #[serde(default)]
    pub write_rejected: bool,
    #[serde(default)]
    pub rejected_path: Option<String>,
}

impl OutputConfig {
    pub fn rejected_path(&self) -> String {
        self.rejected_path.clone().unwrap_or_else(|| format!("{}.rejected.jsonl", self.path))
    }
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            format: default_format(),
            path: "./out/dataset.jsonl".to_string(),
            write_rejected: false,
            rejected_path: None,
        }
    }
}
