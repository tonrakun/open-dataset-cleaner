use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Jsonl,
    Parquet,
}

fn default_format() -> OutputFormat {
    OutputFormat::default()
}

#[derive(Debug, Clone, Deserialize)]
pub struct OutputConfig {
    #[serde(default = "default_format")]
    pub format: OutputFormat,
    pub path: String,
    #[serde(default)]
    pub write_rejected: bool,
    #[serde(default)]
    pub rejected_path: Option<String>,
}

impl OutputConfig {
    pub fn rejected_path(&self) -> String {
        self.rejected_path.clone().unwrap_or_else(|| {
            let ext = match self.format {
                OutputFormat::Jsonl => "jsonl",
                OutputFormat::Parquet => "parquet",
            };
            format!("{}.rejected.{}", self.path, ext)
        })
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
