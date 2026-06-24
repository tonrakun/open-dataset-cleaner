use crate::config::{Condition, ConditionValue, FiltersConfig, Op, Rule, ScoringConfig};
use crate::record::RejectionReason;
use crate::scoring::ScoreSet;

/// 閾値に基づき採用/除外を判定する。複数の条件に該当する場合は最初に検出した理由を返す。
pub fn evaluate(
    scores: &ScoreSet,
    scoring: &ScoringConfig,
    filters: &FiltersConfig,
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
    if filters.thresholds.reject_on_residual_html && scores.has_residual_html {
        return Err(RejectionReason::ResidualHtmlDetected);
    }
    if filters.thresholds.reject_on_residual_url && scores.has_residual_url {
        return Err(RejectionReason::ResidualUrlDetected);
    }
    for rule in &filters.rules {
        if evaluate_rule(&rule.when, scores) {
            return Err(RejectionReason::CustomRule(rule.name.clone()));
        }
    }
    Ok(())
}

fn evaluate_rule(rule: &Rule, scores: &ScoreSet) -> bool {
    match rule {
        Rule::All { all } => all.iter().all(|r| evaluate_rule(r, scores)),
        Rule::Any { any } => any.iter().any(|r| evaluate_rule(r, scores)),
        Rule::Not { not } => !evaluate_rule(not, scores),
        Rule::Cond(cond) => evaluate_condition(cond, scores),
    }
}

fn evaluate_condition(cond: &Condition, scores: &ScoreSet) -> bool {
    match cond.field.as_str() {
        "detected_language" => {
            let lang = scores.detected_language.as_deref();
            match (&cond.op, &cond.value) {
                (Op::In, ConditionValue::List(list)) => lang
                    .map(|l| list.iter().any(|a| a.eq_ignore_ascii_case(l)))
                    .unwrap_or(false),
                (Op::NotIn, ConditionValue::List(list)) => lang
                    .map(|l| !list.iter().any(|a| a.eq_ignore_ascii_case(l)))
                    .unwrap_or(true),
                (Op::Eq, ConditionValue::Text(text)) => {
                    lang.map(|l| l.eq_ignore_ascii_case(text)).unwrap_or(false)
                }
                (Op::Ne, ConditionValue::Text(text)) => {
                    lang.map(|l| !l.eq_ignore_ascii_case(text)).unwrap_or(true)
                }
                _ => false,
            }
        }
        "has_residual_html" => compare_bool(cond, scores.has_residual_html),
        "has_residual_url" => compare_bool(cond, scores.has_residual_url),
        "language_mixed_ratio" => compare_number(cond, scores.language_mixed_ratio),
        "duplicate_line_ratio" => compare_number(cond, scores.duplicate_line_ratio),
        "symbol_digit_ratio" => compare_number(cond, scores.symbol_digit_ratio),
        "avg_sentence_length" => compare_number(cond, scores.avg_sentence_length),
        "sentence_length_variance" => compare_number(cond, scores.sentence_length_variance),
        "hiragana_ratio" => compare_number(cond, scores.char_ratios.hiragana),
        "katakana_ratio" => compare_number(cond, scores.char_ratios.katakana),
        "kanji_ratio" => compare_number(cond, scores.char_ratios.kanji),
        "alnum_ratio" => compare_number(cond, scores.char_ratios.alnum),
        "other_ratio" => compare_number(cond, scores.char_ratios.other),
        field if field.starts_with("plugin:") => {
            let key = &field["plugin:".len()..];
            scores.plugin_scores.get(key).map(|v| compare_number(cond, *v)).unwrap_or(false)
        }
        _ => false,
    }
}

fn compare_number(cond: &Condition, actual: f64) -> bool {
    let expected = match &cond.value {
        ConditionValue::Number(n) => *n,
        _ => return false,
    };
    match cond.op {
        Op::Lt => actual < expected,
        Op::Lte => actual <= expected,
        Op::Gt => actual > expected,
        Op::Gte => actual >= expected,
        Op::Eq => actual == expected,
        Op::Ne => actual != expected,
        Op::In | Op::NotIn => false,
    }
}

fn compare_bool(cond: &Condition, actual: bool) -> bool {
    let expected = match &cond.value {
        ConditionValue::Bool(b) => *b,
        _ => return false,
    };
    match cond.op {
        Op::Eq => actual == expected,
        Op::Ne => actual != expected,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{LanguageScoringConfig, NamedRule, TextQualityScoringConfig};

    fn base_scoring() -> ScoringConfig {
        ScoringConfig::default()
    }

    fn base_filters() -> FiltersConfig {
        FiltersConfig::default()
    }

    #[test]
    fn accepts_when_no_thresholds_configured() {
        let scores = ScoreSet::default();
        assert!(evaluate(&scores, &base_scoring(), &base_filters()).is_ok());
    }

    #[test]
    fn rejects_disallowed_language() {
        let mut scoring = base_scoring();
        scoring.language = LanguageScoringConfig { allow: vec!["ja".into()], max_mixed_ratio: None };
        let scores = ScoreSet { detected_language: Some("en".to_string()), ..Default::default() };
        let result = evaluate(&scores, &scoring, &base_filters());
        assert!(matches!(result, Err(RejectionReason::LanguageNotAllowed)));
    }

    #[test]
    fn rejects_exceeded_mixed_ratio() {
        let mut scoring = base_scoring();
        scoring.language = LanguageScoringConfig { allow: vec![], max_mixed_ratio: Some(0.1) };
        let scores = ScoreSet { language_mixed_ratio: 0.5, ..Default::default() };
        let result = evaluate(&scores, &scoring, &base_filters());
        assert!(matches!(result, Err(RejectionReason::MixedLanguageRatioExceeded)));
    }

    #[test]
    fn rejects_residual_html_when_configured() {
        let scores = ScoreSet { has_residual_html: true, ..Default::default() };
        let mut filters = base_filters();
        filters.thresholds.reject_on_residual_html = true;
        let result = evaluate(&scores, &base_scoring(), &filters);
        assert!(matches!(result, Err(RejectionReason::ResidualHtmlDetected)));
    }

    #[test]
    fn ignores_residual_html_when_not_configured() {
        let scores = ScoreSet { has_residual_html: true, ..Default::default() };
        let result = evaluate(&scores, &base_scoring(), &base_filters());
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
        let result = evaluate(&scores, &scoring, &base_filters());
        assert!(matches!(result, Err(RejectionReason::SymbolRatioExceeded)));
    }

    #[test]
    fn rejects_via_and_combination_rule() {
        let mut filters = base_filters();
        filters.rules.push(NamedRule {
            name: "junk_combo".to_string(),
            when: toml::from_str(
                r#"
                all = [
                    { field = "symbol_digit_ratio", op = "gt", value = 0.2 },
                    { field = "duplicate_line_ratio", op = "gt", value = 0.3 },
                ]
                "#,
            )
            .unwrap(),
        });
        let scores =
            ScoreSet { symbol_digit_ratio: 0.5, duplicate_line_ratio: 0.4, ..Default::default() };
        let result = evaluate(&scores, &base_scoring(), &filters);
        assert!(matches!(result, Err(RejectionReason::CustomRule(name)) if name == "junk_combo"));
    }

    #[test]
    fn and_combination_rule_requires_all_conditions() {
        let mut filters = base_filters();
        filters.rules.push(NamedRule {
            name: "junk_combo".to_string(),
            when: toml::from_str(
                r#"
                all = [
                    { field = "symbol_digit_ratio", op = "gt", value = 0.2 },
                    { field = "duplicate_line_ratio", op = "gt", value = 0.3 },
                ]
                "#,
            )
            .unwrap(),
        });
        let scores =
            ScoreSet { symbol_digit_ratio: 0.5, duplicate_line_ratio: 0.1, ..Default::default() };
        let result = evaluate(&scores, &base_scoring(), &filters);
        assert!(result.is_ok());
    }

    #[test]
    fn rejects_via_or_and_not_combination_rule() {
        let mut filters = base_filters();
        filters.rules.push(NamedRule {
            name: "lang_or_residual".to_string(),
            when: toml::from_str(
                r#"
                any = [
                    { not = { field = "detected_language", op = "in", value = ["ja", "en"] } },
                    { field = "has_residual_html", op = "eq", value = true },
                ]
                "#,
            )
            .unwrap(),
        });
        let scores = ScoreSet { detected_language: Some("fr".to_string()), ..Default::default() };
        let result = evaluate(&scores, &base_scoring(), &filters);
        assert!(matches!(result, Err(RejectionReason::CustomRule(name)) if name == "lang_or_residual"));
    }

    #[test]
    fn rejects_via_plugin_score_field() {
        let mut filters = base_filters();
        filters.rules.push(NamedRule {
            name: "ad_score_high".to_string(),
            when: toml::from_str("field = \"plugin:ad_detector\"\nop = \"gt\"\nvalue = 0.8\n").unwrap(),
        });
        let mut scores = ScoreSet::default();
        scores.plugin_scores.insert("ad_detector".to_string(), 0.95);
        let result = evaluate(&scores, &base_scoring(), &filters);
        assert!(matches!(result, Err(RejectionReason::CustomRule(name)) if name == "ad_score_high"));
    }

    #[test]
    fn ignores_missing_plugin_score() {
        let mut filters = base_filters();
        filters.rules.push(NamedRule {
            name: "ad_score_high".to_string(),
            when: toml::from_str("field = \"plugin:ad_detector\"\nop = \"gt\"\nvalue = 0.8\n").unwrap(),
        });
        let scores = ScoreSet::default();
        let result = evaluate(&scores, &base_scoring(), &filters);
        assert!(result.is_ok());
    }
}
