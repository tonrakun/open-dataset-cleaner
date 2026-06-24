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
    ExtractionError(String),
}

impl RejectionReason {
    pub fn as_key(&self) -> &'static str {
        match self {
            RejectionReason::LanguageNotAllowed => "language_not_allowed",
            RejectionReason::MixedLanguageRatioExceeded => "mixed_language_ratio_exceeded",
            RejectionReason::DuplicateLineRatioExceeded => "duplicate_line_ratio_exceeded",
            RejectionReason::SymbolRatioExceeded => "symbol_ratio_exceeded",
            RejectionReason::ResidualHtmlDetected => "residual_html_detected",
            RejectionReason::ResidualUrlDetected => "residual_url_detected",
            RejectionReason::ExtractionError(_) => "extraction_error",
        }
    }
}

pub enum RecordOutcome {
    Accepted { record: RawRecord, scores: ScoreSet },
    Rejected { record: RawRecord, scores: ScoreSet, reason: RejectionReason },
    Error { source_path: std::path::PathBuf, message: String },
}
