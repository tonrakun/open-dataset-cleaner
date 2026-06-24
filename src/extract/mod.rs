pub mod validate;

pub use validate::{validate_extracted_text, ExtractionValidationReport};

use crate::record::RawRecord;

pub trait Extractor: Send + Sync {
    fn extract(&self, raw: &RawRecord) -> anyhow::Result<String>;
}

/// M1ではHTML抽出を行わず、入力テキストをそのまま通過させる（トリムのみ）。
pub struct PassthroughExtractor;

impl Extractor for PassthroughExtractor {
    fn extract(&self, raw: &RawRecord) -> anyhow::Result<String> {
        Ok(raw.text.trim().to_string())
    }
}
