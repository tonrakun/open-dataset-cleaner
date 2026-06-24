use super::{CharRatios, ScoreSet, Scorer};

pub struct CharRatioScorer;

fn classify(c: char) -> &'static str {
    let cp = c as u32;
    if (0x3040..=0x309F).contains(&cp) {
        "hiragana"
    } else if (0x30A0..=0x30FF).contains(&cp) {
        "katakana"
    } else if (0x4E00..=0x9FFF).contains(&cp) {
        "kanji"
    } else if c.is_ascii_alphanumeric() {
        "alnum"
    } else {
        "other"
    }
}

pub fn compute_char_ratios(text: &str) -> CharRatios {
    let total = text.chars().count();
    if total == 0 {
        return CharRatios::default();
    }
    let mut hiragana = 0usize;
    let mut katakana = 0usize;
    let mut kanji = 0usize;
    let mut alnum = 0usize;
    let mut other = 0usize;
    for c in text.chars() {
        match classify(c) {
            "hiragana" => hiragana += 1,
            "katakana" => katakana += 1,
            "kanji" => kanji += 1,
            "alnum" => alnum += 1,
            _ => other += 1,
        }
    }
    let total = total as f64;
    CharRatios {
        hiragana: hiragana as f64 / total,
        katakana: katakana as f64 / total,
        kanji: kanji as f64 / total,
        alnum: alnum as f64 / total,
        other: other as f64 / total,
    }
}

impl Scorer for CharRatioScorer {
    fn name(&self) -> &'static str {
        "char_ratio"
    }

    fn score(&self, text: &str, scores: &mut ScoreSet) -> anyhow::Result<()> {
        scores.char_ratios = compute_char_ratios(text);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_yields_zero_ratios() {
        let r = compute_char_ratios("");
        assert_eq!(r.hiragana, 0.0);
        assert_eq!(r.other, 0.0);
    }

    #[test]
    fn classifies_hiragana_katakana_kanji_alnum_other() {
        let r = compute_char_ratios("あアA漢 !");
        assert!((r.hiragana - 1.0 / 6.0).abs() < 1e-9);
        assert!((r.katakana - 1.0 / 6.0).abs() < 1e-9);
        assert!((r.kanji - 1.0 / 6.0).abs() < 1e-9);
        assert!((r.alnum - 1.0 / 6.0).abs() < 1e-9);
        // space + '!' は other
        assert!((r.other - 2.0 / 6.0).abs() < 1e-9);
    }
}
