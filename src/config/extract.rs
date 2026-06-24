use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ExtractConfig {
    #[serde(default = "default_true")]
    pub validate_residual: bool,
}

fn default_true() -> bool {
    true
}

impl Default for ExtractConfig {
    fn default() -> Self {
        Self { validate_residual: true }
    }
}
