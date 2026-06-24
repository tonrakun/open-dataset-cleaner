use super::RecordSource;
use crate::record::RawRecord;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

pub struct JsonlSource {
    source_path: PathBuf,
    reader: BufReader<File>,
    text_field: String,
    meta_fields: Vec<String>,
    line_no: u64,
}

impl JsonlSource {
    pub fn open(path: &Path, text_field: String, meta_fields: Vec<String>) -> anyhow::Result<Self> {
        let file = File::open(path)
            .map_err(|e| anyhow::anyhow!("ファイルを開けません {}: {}", path.display(), e))?;
        Ok(Self {
            source_path: path.to_path_buf(),
            reader: BufReader::new(file),
            text_field,
            meta_fields,
            line_no: 0,
        })
    }
}

impl RecordSource for JsonlSource {
    fn next_record(&mut self) -> anyhow::Result<Option<RawRecord>> {
        loop {
            let mut line = String::new();
            let bytes = self.reader.read_line(&mut line)?;
            if bytes == 0 {
                return Ok(None);
            }
            self.line_no += 1;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let value: serde_json::Value = match serde_json::from_str(trimmed) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(
                        "{}:{} のJSON解析に失敗したためスキップします: {}",
                        self.source_path.display(),
                        self.line_no,
                        e
                    );
                    continue;
                }
            };

            let text = match value.get(&self.text_field).and_then(|v| v.as_str()) {
                Some(t) => t.to_string(),
                None => {
                    tracing::warn!(
                        "{}:{} にフィールド '{}' がないためスキップします",
                        self.source_path.display(),
                        self.line_no,
                        self.text_field
                    );
                    continue;
                }
            };

            let mut meta = serde_json::Map::new();
            for field in &self.meta_fields {
                if let Some(v) = value.get(field) {
                    meta.insert(field.clone(), v.clone());
                }
            }

            return Ok(Some(RawRecord {
                id: format!("{}:{}", self.source_path.display(), self.line_no),
                source_path: self.source_path.clone(),
                text,
                meta,
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn reads_text_and_meta_fields() {
        let f = write_temp(
            "{\"text\": \"hello\", \"url\": \"http://a\", \"id\": 1}\n{\"text\": \"world\"}\n",
        );
        let mut src = JsonlSource::open(f.path(), "text".to_string(), vec!["url".to_string()]).unwrap();
        let r1 = src.next_record().unwrap().unwrap();
        assert_eq!(r1.text, "hello");
        assert_eq!(r1.meta.get("url").unwrap(), "http://a");
        let r2 = src.next_record().unwrap().unwrap();
        assert_eq!(r2.text, "world");
        assert!(r2.meta.get("url").is_none());
        assert!(src.next_record().unwrap().is_none());
    }

    #[test]
    fn skips_malformed_lines_and_missing_field() {
        let f = write_temp("not json\n{\"other\": 1}\n{\"text\": \"ok\"}\n");
        let mut src = JsonlSource::open(f.path(), "text".to_string(), vec![]).unwrap();
        let r = src.next_record().unwrap().unwrap();
        assert_eq!(r.text, "ok");
        assert!(src.next_record().unwrap().is_none());
    }
}
