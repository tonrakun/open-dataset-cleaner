use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ExtractConfig {
    #[serde(default = "default_true")]
    pub validate_residual: bool,
    /// HTML/WARC入力時にnav/footer/広告classなどのボイラープレートを除去する。
    #[serde(default = "default_true")]
    pub strip_boilerplate: bool,
    /// HTML/WARC入力の本文をMarkdownに変換する（false時はプレーンテキスト抽出）。
    #[serde(default)]
    pub markdown: bool,
    #[serde(default = "default_true")]
    pub markdown_keep_headings: bool,
    #[serde(default = "default_true")]
    pub markdown_keep_lists: bool,
    #[serde(default)]
    pub markdown_keep_links: bool,
}

fn default_true() -> bool {
    true
}

impl Default for ExtractConfig {
    fn default() -> Self {
        Self {
            validate_residual: true,
            strip_boilerplate: true,
            markdown: false,
            markdown_keep_headings: true,
            markdown_keep_lists: true,
            markdown_keep_links: false,
        }
    }
}
