use crate::config::{ScoringConfig, ThresholdConfig};
use crate::record::RejectionReason;
use crate::scoring::ScoreSet;

/// 閾値に基づき採用/除外を判定する。複数の条件に該当する場合は最初に検出した理由を返す。
pub fn evaluate(
    scores: &ScoreSet,
    scoring: &ScoringConfig,
    thresholds: &ThresholdConfig,
) -> Result<(), RejectionReason> {
    if !scoring.language.allow.is_empty() {
        let allowed = scores
            .detected_language
            .as_deref()
            .map(|lang| scoring.language.allow.iter().any(|a| a.eq_ignore_ascii_case(lang)))
            .unwrap_or(false);
        if !allowed {
            return Err(RejectionReason::LanguageNotAllowed);
        }
    }
    if let Some(max) = scoring.language.max_mixed_ratio {
        if scores.language_mixed_ratio > max {
            return Err(RejectionReason::MixedLanguageRatioExceeded);
        }
    }
    if let Some(max) = scoring.text_quality.max_duplicate_line_ratio {
        if scores.duplicate_line_ratio > max {
            return Err(RejectionReason::DuplicateLineRatioExceeded);
        }
    }
    if let Some(max) = scoring.text_quality.max_symbol_ratio {
        if scores.symbol_digit_ratio > max {
            return Err(RejectionReason::SymbolRatioExceeded);
        }
    }
    if thresholds.reject_on_residual_html && scores.has_residual_html {
        return Err(RejectionReason::ResidualHtmlDetected);
    }
    if thresholds.reject_on_residual_url && scores.has_residual_url {
        return Err(RejectionReason::ResidualUrlDetected);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{LanguageScoringConfig, TextQualityScoringConfig};

    fn base_scoring() -> ScoringConfig {
        ScoringConfig::default()
    }

    fn base_thresholds() -> ThresholdConfig {
        ThresholdConfig::default()
    }

    #[test]
    fn accepts_when_no_thresholds_configured() {
        let scores = ScoreSet::default();
        assert!(evaluate(&scores, &base_scoring(), &base_thresholds()).is_ok());
    }

    #[test]
    fn rejects_disallowed_language() {
        let mut scoring = base_scoring();
        scoring.language = LanguageScoringConfig { allow: vec!["ja".into()], max_mixed_ratio: None };
        let scores = ScoreSet { detected_language: Some("en".to_string()), ..Default::default() };
        let result = evaluate(&scores, &scoring, &base_thresholds());
        assert!(matches!(result, Err(RejectionReason::LanguageNotAllowed)));
    }

    #[test]
    fn rejects_exceeded_mixed_ratio() {
        let mut scoring = base_scoring();
        scoring.language = LanguageScoringConfig { allow: vec![], max_mixed_ratio: Some(0.1) };
        let scores = ScoreSet { language_mixed_ratio: 0.5, ..Default::default() };
        let result = evaluate(&scores, &scoring, &base_thresholds());
        assert!(matches!(result, Err(RejectionReason::MixedLanguageRatioExceeded)));
    }

    #[test]
    fn rejects_residual_html_when_configured() {
        let scores = ScoreSet { has_residual_html: true, ..Default::default() };
        let thresholds = ThresholdConfig { reject_on_residual_html: true, reject_on_residual_url: false };
        let result = evaluate(&scores, &base_scoring(), &thresholds);
        assert!(matches!(result, Err(RejectionReason::ResidualHtmlDetected)));
    }

    #[test]
    fn ignores_residual_html_when_not_configured() {
        let scores = ScoreSet { has_residual_html: true, ..Default::default() };
        let result = evaluate(&scores, &base_scoring(), &base_thresholds());
        assert!(result.is_ok());
    }

    #[test]
    fn rejects_exceeded_symbol_ratio() {
        let mut scoring = base_scoring();
        scoring.text_quality = TextQualityScoringConfig {
            max_duplicate_line_ratio: None,
            max_symbol_ratio: Some(0.1),
            perplexity_enabled: false,
        };
        let scores = ScoreSet { symbol_digit_ratio: 0.9, ..Default::default() };
        let result = evaluate(&scores, &scoring, &base_thresholds());
        assert!(matches!(result, Err(RejectionReason::SymbolRatioExceeded)));
    }
}
