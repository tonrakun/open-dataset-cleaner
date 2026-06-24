mod outcome;

pub use outcome::{RecordOutcome, RejectionReason};

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct RawRecord {
    pub id: String,
    pub source_path: PathBuf,
    pub text: String,
    pub meta: serde_json::Map<String, serde_json::Value>,
}
