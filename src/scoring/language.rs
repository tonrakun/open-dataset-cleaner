use super::{ScoreSet, Scorer};
use lingua::{Language, LanguageDetector, LanguageDetectorBuilder};

pub struct LanguageScorer {
    detector: LanguageDetector,
}

impl LanguageScorer {
    /// `codes` はISO 639-1の小文字コード（例: "ja", "en"）。空の場合はサポート対象の全言語を使う。
    pub fn from_allowed_codes(codes: &[String]) -> anyhow::Result<Self> {
        let languages: Vec<Language> = if codes.is_empty() {
            Self::supported_languages()
        } else {
            codes
                .iter()
                .map(|c| {
                    Self::parse_iso_code(c)
                        .ok_or_else(|| anyhow::anyhow!("未サポートの言語コードです: {}", c))
                })
                .collect::<anyhow::Result<Vec<_>>>()?
        };
        let detector = LanguageDetectorBuilder::from_languages(&languages).build();
        Ok(Self { detector })
    }

    pub fn supported_languages() -> Vec<Language> {
        vec![Language::Japanese, Language::English]
    }

    pub fn parse_iso_code(code: &str) -> Option<Language> {
        match code.to_lowercase().as_str() {
            "ja" => Some(Language::Japanese),
            "en" => Some(Language::English),
            _ => None,
        }
    }
}

impl Scorer for LanguageScorer {
    fn name(&self) -> &'static str {
        "language"
    }

    fn score(&self, text: &str, scores: &mut ScoreSet) -> anyhow::Result<()> {
        let total_chars = text.chars().count();
        let main_lang = self.detector.detect_language_of(text);
        scores.detected_language = main_lang.map(|l| l.iso_code_639_1().to_string().to_lowercase());

        scores.language_mixed_ratio = match (main_lang, total_chars) {
            (_, 0) | (None, _) => 0.0,
            (Some(main), total) => {
                let results = self.detector.detect_multiple_languages_of(text);
                let mismatched_chars: usize = results
                    .iter()
                    .filter(|r| r.language() != main)
                    .map(|r| text[r.start_index()..r.end_index()].chars().count())
                    .sum();
                mismatched_chars as f64 / total as f64
            }
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_japanese_and_english() {
        let scorer = LanguageScorer::from_allowed_codes(&[]).unwrap();
        let mut scores = ScoreSet::default();
        scorer.score("これは日本語の文章です。", &mut scores).unwrap();
        assert_eq!(scores.detected_language.as_deref(), Some("ja"));

        let mut scores = ScoreSet::default();
        scorer.score("This is an English sentence.", &mut scores).unwrap();
        assert_eq!(scores.detected_language.as_deref(), Some("en"));
    }

    #[test]
    fn mixed_language_ratio_is_positive_for_mixed_text() {
        let scorer = LanguageScorer::from_allowed_codes(&[]).unwrap();
        let mut scores = ScoreSet::default();
        scorer
            .score("これは日本語です。This is English text mixed in.", &mut scores)
            .unwrap();
        assert!(scores.language_mixed_ratio > 0.0);
    }

    #[test]
    fn empty_text_yields_zero_mixed_ratio() {
        let scorer = LanguageScorer::from_allowed_codes(&[]).unwrap();
        let mut scores = ScoreSet::default();
        scorer.score("", &mut scores).unwrap();
        assert_eq!(scores.language_mixed_ratio, 0.0);
    }

    #[test]
    fn rejects_unknown_iso_code() {
        let result = LanguageScorer::from_allowed_codes(&["xx".to_string()]);
        assert!(result.is_err());
    }
}
