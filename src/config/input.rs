use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputFormat {
    Text,
    Jsonl,
    Warc,
    Html,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlainTextMode {
    #[default]
    NewlineDelimited,
    OneFilePerDocument,
}

fn default_text_field() -> String {
    "text".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct InputConfig {
    pub format: InputFormat,
    pub paths: Vec<String>,
    #[serde(default)]
    pub text_mode: PlainTextMode,
    #[serde(default = "default_text_field")]
    pub text_field: String,
    #[serde(default)]
    pub meta_fields: Vec<String>,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            format: InputFormat::Jsonl,
            paths: Vec::new(),
            text_mode: PlainTextMode::default(),
            text_field: default_text_field(),
            meta_fields: Vec::new(),
        }
    }
}
