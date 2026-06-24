use super::Extractor;
use crate::record::RawRecord;
use ego_tree::NodeRef;
use once_cell::sync::Lazy;
use regex::Regex;
use scraper::{Html, Node, Selector};

/// Markdown変換時の見出し・リスト・リンクの保持/除去オプション。
#[derive(Debug, Clone, Copy)]
pub struct MarkdownOptions {
    pub keep_headings: bool,
    pub keep_lists: bool,
    pub keep_links: bool,
}

/// HTMLを解析し、ボイラープレート除去・本文抽出・(任意で)Markdown変換を行う `Extractor`。
/// 完全なreadability系density scoringではなく、`<article>`/`<main>`優先選択 + タグ/クラス名による
/// 簡易ヒューリスティックでボイラープレートを除去する簡易版。
pub struct HtmlExtractor {
    strip_boilerplate: bool,
    markdown: Option<MarkdownOptions>,
}

impl HtmlExtractor {
    pub fn new(strip_boilerplate: bool, markdown: Option<MarkdownOptions>) -> Self {
        Self { strip_boilerplate, markdown }
    }

    pub fn from_config(cfg: &crate::config::ExtractConfig) -> Self {
        let markdown = if cfg.markdown {
            Some(MarkdownOptions {
                keep_headings: cfg.markdown_keep_headings,
                keep_lists: cfg.markdown_keep_lists,
                keep_links: cfg.markdown_keep_links,
            })
        } else {
            None
        };
        Self::new(cfg.strip_boilerplate, markdown)
    }
}

impl Extractor for HtmlExtractor {
    fn extract(&self, raw: &RawRecord) -> anyhow::Result<String> {
        let document = Html::parse_document(&raw.text);
        let root = select_content_root(&document);
        let mut out = String::new();
        render_node(root, self.strip_boilerplate, self.markdown.as_ref(), &mut out);
        Ok(normalize_whitespace(&out))
    }
}

static ARTICLE_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse("article").unwrap());
static MAIN_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse("main").unwrap());
static BODY_SEL: Lazy<Selector> = Lazy::new(|| Selector::parse("body").unwrap());

fn select_content_root(document: &Html) -> NodeRef<'_, Node> {
    if let Some(el) = document.select(&ARTICLE_SEL).next() {
        return *el;
    }
    if let Some(el) = document.select(&MAIN_SEL).next() {
        return *el;
    }
    if let Some(el) = document.select(&BODY_SEL).next() {
        return *el;
    }
    *document.root_element()
}

const ALWAYS_SKIP_TAGS: &[&str] = &["script", "style", "noscript", "head", "title", "meta", "link", "svg"];
const BOILERPLATE_TAGS: &[&str] = &["nav", "footer", "header", "aside", "form", "iframe"];

static BOILERPLATE_CLASS_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(nav|menu|footer|sidebar|ads?|advert(?:isement)?|comment|social|breadcrumb|cookie|popup|share)\b")
        .unwrap()
});

fn is_boilerplate(elem: &scraper::node::Element) -> bool {
    if BOILERPLATE_TAGS.contains(&elem.name()) {
        return true;
    }
    let class_attr = elem.attr("class").unwrap_or("");
    let id_attr = elem.attr("id").unwrap_or("");
    BOILERPLATE_CLASS_RE.is_match(class_attr) || BOILERPLATE_CLASS_RE.is_match(id_attr)
}

fn render_node(node: NodeRef<'_, Node>, strip_boilerplate: bool, markdown: Option<&MarkdownOptions>, out: &mut String) {
    match node.value() {
        Node::Text(text) => out.push_str(&text.text),
        Node::Element(elem) => {
            let tag = elem.name();
            if ALWAYS_SKIP_TAGS.contains(&tag) {
                return;
            }
            if strip_boilerplate && is_boilerplate(elem) {
                return;
            }
            match tag {
                "br" => out.push('\n'),
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    ensure_blank_line(out);
                    if markdown.map(|m| m.keep_headings).unwrap_or(false) {
                        let level: usize = tag[1..].parse().unwrap_or(1);
                        out.push_str(&"#".repeat(level));
                        out.push(' ');
                    }
                    render_children(node, strip_boilerplate, markdown, out);
                    ensure_blank_line(out);
                }
                "ul" | "ol" => {
                    ensure_blank_line(out);
                    render_list(node, tag == "ol", strip_boilerplate, markdown, out);
                    ensure_blank_line(out);
                }
                "a" => {
                    if markdown.map(|m| m.keep_links).unwrap_or(false) {
                        if let Some(href) = elem.attr("href") {
                            let mut inner = String::new();
                            render_children(node, strip_boilerplate, markdown, &mut inner);
                            let inner = inner.trim();
                            if inner.is_empty() {
                                return;
                            }
                            out.push_str(&format!("[{}]({})", inner, href));
                            return;
                        }
                    }
                    render_children(node, strip_boilerplate, markdown, out);
                }
                "p" | "div" | "section" | "article" | "main" | "blockquote" | "pre" | "table" | "tr" => {
                    ensure_blank_line(out);
                    render_children(node, strip_boilerplate, markdown, out);
                    ensure_blank_line(out);
                }
                _ => render_children(node, strip_boilerplate, markdown, out),
            }
        }
        _ => {}
    }
}

fn render_children(node: NodeRef<'_, Node>, strip_boilerplate: bool, markdown: Option<&MarkdownOptions>, out: &mut String) {
    for child in node.children() {
        render_node(child, strip_boilerplate, markdown, out);
    }
}

fn render_list(node: NodeRef<'_, Node>, ordered: bool, strip_boilerplate: bool, markdown: Option<&MarkdownOptions>, out: &mut String) {
    let keep_lists = markdown.map(|m| m.keep_lists).unwrap_or(false);
    let mut counter = 0usize;
    for child in node.children() {
        if let Node::Element(elem) = child.value() {
            if elem.name() != "li" {
                continue;
            }
            if strip_boilerplate && is_boilerplate(elem) {
                continue;
            }
            counter += 1;
            if keep_lists {
                if ordered {
                    out.push_str(&format!("{}. ", counter));
                } else {
                    out.push_str("- ");
                }
            }
            render_children(child, strip_boilerplate, markdown, out);
            out.push('\n');
        }
    }
}

fn ensure_blank_line(out: &mut String) {
    if out.is_empty() {
        return;
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.ends_with("\n\n") {
        out.push('\n');
    }
}

static INLINE_SPACE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[ \t]+").unwrap());
static EXCESS_NEWLINE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\n{3,}").unwrap());

fn normalize_whitespace(input: &str) -> String {
    let collapsed_spaces = INLINE_SPACE_RE.replace_all(input, " ");
    let collapsed_newlines = EXCESS_NEWLINE_RE.replace_all(&collapsed_spaces, "\n\n");
    let lines: Vec<&str> = collapsed_newlines.lines().map(str::trim).collect();
    lines.join("\n").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::RawRecord;
    use std::path::PathBuf;

    fn record(html: &str) -> RawRecord {
        RawRecord {
            id: "test".to_string(),
            source_path: PathBuf::from("test.html"),
            text: html.to_string(),
            meta: Default::default(),
        }
    }

    #[test]
    fn strips_boilerplate_and_extracts_article_text() {
        let html = r#"
            <html><body>
                <nav>home about</nav>
                <header class="site-header">Logo</header>
                <article>
                    <h1>Title</h1>
                    <p>Main content here.</p>
                </article>
                <div class="ad-banner">Buy now!</div>
                <footer>copyright</footer>
            </body></html>
        "#;
        let extractor = HtmlExtractor::new(true, None);
        let text = extractor.extract(&record(html)).unwrap();
        assert!(text.contains("Title"));
        assert!(text.contains("Main content here."));
        assert!(!text.contains("home about"));
        assert!(!text.contains("Buy now"));
        assert!(!text.contains("copyright"));
    }

    #[test]
    fn prefers_article_over_surrounding_body_text() {
        let html = r#"<html><body><div>sidebar junk</div><article><p>real content</p></article></body></html>"#;
        let extractor = HtmlExtractor::new(false, None);
        let text = extractor.extract(&record(html)).unwrap();
        assert!(text.contains("real content"));
    }

    #[test]
    fn markdown_mode_keeps_headings_lists_and_links_when_enabled() {
        let html = r#"<article><h2>Heading</h2><ul><li>one</li><li>two</li></ul><p>see <a href="https://example.com">link</a></p></article>"#;
        let opts = MarkdownOptions { keep_headings: true, keep_lists: true, keep_links: true };
        let extractor = HtmlExtractor::new(true, Some(opts));
        let text = extractor.extract(&record(html)).unwrap();
        assert!(text.contains("## Heading"));
        assert!(text.contains("- one"));
        assert!(text.contains("- two"));
        assert!(text.contains("[link](https://example.com)"));
    }

    #[test]
    fn markdown_mode_strips_markup_when_options_disabled() {
        let html = r#"<article><h2>Heading</h2><ul><li>one</li></ul><p>see <a href="https://example.com">link</a></p></article>"#;
        let opts = MarkdownOptions { keep_headings: false, keep_lists: false, keep_links: false };
        let extractor = HtmlExtractor::new(true, Some(opts));
        let text = extractor.extract(&record(html)).unwrap();
        assert!(!text.contains('#'));
        assert!(!text.contains('-'));
        assert!(!text.contains('['));
        assert!(text.contains("Heading"));
        assert!(text.contains("one"));
        assert!(text.contains("link"));
    }
}
