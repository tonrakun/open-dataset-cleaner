use crate::record::RawRecord;
use crate::scoring::ScoreSet;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectionReason {
    LanguageNotAllowed,
    MixedLanguageRatioExceeded,
    DuplicateLineRatioExceeded,
    SymbolRatioExceeded,
    ResidualHtmlDetected,
    ResidualUrlDetected,
    DuplicateExact,
    DuplicateNearDuplicate,
    ExtractionError(String),
    CustomRule(String),
}

impl RejectionReason {
    pub fn as_key(&self) -> String {
        match self {
            RejectionReason::LanguageNotAllowed => "language_not_allowed".to_string(),
            RejectionReason::MixedLanguageRatioExceeded => "mixed_language_ratio_exceeded".to_string(),
            RejectionReason::DuplicateLineRatioExceeded => "duplicate_line_ratio_exceeded".to_string(),
            RejectionReason::SymbolRatioExceeded => "symbol_ratio_exceeded".to_string(),
            RejectionReason::ResidualHtmlDetected => "residual_html_detected".to_string(),
            RejectionReason::ResidualUrlDetected => "residual_url_detected".to_string(),
            RejectionReason::DuplicateExact => "duplicate_exact".to_string(),
            RejectionReason::DuplicateNearDuplicate => "duplicate_near_duplicate".to_string(),
            RejectionReason::ExtractionError(_) => "extraction_error".to_string(),
            RejectionReason::CustomRule(name) => format!("custom_rule:{}", name),
        }
    }
}

pub enum RecordOutcome {
    Accepted { record: RawRecord, scores: ScoreSet },
    Rejected { record: RawRecord, scores: ScoreSet, reason: RejectionReason },
    Error { source_path: std::path::PathBuf, message: String },
}
