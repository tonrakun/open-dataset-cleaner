use super::RecordSource;
use crate::record::RawRecord;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

/// HTMLファイル1件を1レコードとして読み込む `RecordSource`。
/// 複数ファイル/ディレクトリ/globの展開は `discover_input_files` 側が担う。
pub struct HtmlFileSource {
    source_path: PathBuf,
    consumed: bool,
}

impl HtmlFileSource {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        if !path.is_file() {
            anyhow::bail!("ファイルを開けません {}: ファイルが存在しません", path.display());
        }
        Ok(Self {
            source_path: path.to_path_buf(),
            consumed: false,
        })
    }
}

impl RecordSource for HtmlFileSource {
    fn next_record(&mut self) -> anyhow::Result<Option<RawRecord>> {
        if self.consumed {
            return Ok(None);
        }
        self.consumed = true;

        let mut file = File::open(&self.source_path)
            .map_err(|e| anyhow::anyhow!("ファイルを開けません {}: {}", self.source_path.display(), e))?;
        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|e| anyhow::anyhow!("ファイルを読み込めません {}: {}", self.source_path.display(), e))?;
        if content.trim().is_empty() {
            return Ok(None);
        }

        Ok(Some(RawRecord {
            id: self.source_path.display().to_string(),
            source_path: self.source_path.clone(),
            text: content,
            meta: Default::default(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::Builder::new().suffix(".html").tempfile().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn reads_whole_html_file_as_single_record() {
        let f = write_temp("<html><body><p>hello</p></body></html>");
        let mut src = HtmlFileSource::open(f.path()).unwrap();
        let record = src.next_record().unwrap().unwrap();
        assert!(record.text.contains("<p>hello</p>"));
        assert!(src.next_record().unwrap().is_none());
    }

    #[test]
    fn empty_file_yields_no_records() {
        let f = write_temp("");
        let mut src = HtmlFileSource::open(f.path()).unwrap();
        assert!(src.next_record().unwrap().is_none());
    }
}
