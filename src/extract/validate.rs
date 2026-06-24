use once_cell::sync::Lazy;
use regex::Regex;

static HTML_TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"</?[a-zA-Z][a-zA-Z0-9]*[^>]*>").unwrap());
static URL_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"https?://[^\s]+").unwrap());

#[derive(Debug, Clone, Default)]
pub struct ExtractionValidationReport {
    pub has_residual_html_tags: bool,
    pub residual_html_tag_samples: Vec<String>,
    pub has_residual_urls: bool,
    pub residual_url_samples: Vec<String>,
}

const MAX_SAMPLES: usize = 5;

pub fn validate_extracted_text(text: &str) -> ExtractionValidationReport {
    let html_samples: Vec<String> = HTML_TAG_RE
        .find_iter(text)
        .take(MAX_SAMPLES)
        .map(|m| m.as_str().to_string())
        .collect();
    let url_samples: Vec<String> = URL_RE
        .find_iter(text)
        .take(MAX_SAMPLES)
        .map(|m| m.as_str().to_string())
        .collect();

    ExtractionValidationReport {
        has_residual_html_tags: !html_samples.is_empty(),
        residual_html_tag_samples: html_samples,
        has_residual_urls: !url_samples.is_empty(),
        residual_url_samples: url_samples,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_text_has_no_residuals() {
        let report = validate_extracted_text("これはクリーンなテキストです。");
        assert!(!report.has_residual_html_tags);
        assert!(!report.has_residual_urls);
    }

    #[test]
    fn detects_html_tags_and_urls() {
        let report = validate_extracted_text("<p>see https://example.com here</p>");
        assert!(report.has_residual_html_tags);
        assert_eq!(report.residual_html_tag_samples, vec!["<p>", "</p>"]);
        assert!(report.has_residual_urls);
        assert_eq!(report.residual_url_samples, vec!["https://example.com"]);
    }
}
