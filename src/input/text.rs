use super::RecordSource;
use crate::config::PlainTextMode;
use crate::record::RawRecord;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

pub struct PlainTextSource {
    source_path: PathBuf,
    mode: PlainTextMode,
    reader: BufReader<File>,
    line_no: u64,
    whole_file_consumed: bool,
}

impl PlainTextSource {
    pub fn open(path: &Path, mode: PlainTextMode) -> anyhow::Result<Self> {
        let file = File::open(path)
            .map_err(|e| anyhow::anyhow!("ファイルを開けません {}: {}", path.display(), e))?;
        Ok(Self {
            source_path: path.to_path_buf(),
            mode,
            reader: BufReader::new(file),
            line_no: 0,
            whole_file_consumed: false,
        })
    }
}

impl RecordSource for PlainTextSource {
    fn next_record(&mut self) -> anyhow::Result<Option<RawRecord>> {
        match self.mode {
            PlainTextMode::OneFilePerDocument => {
                if self.whole_file_consumed {
                    return Ok(None);
                }
                self.whole_file_consumed = true;
                let mut content = String::new();
                self.reader.read_to_string(&mut content)?;
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
            PlainTextMode::NewlineDelimited => loop {
                let mut line = String::new();
                let bytes = self.reader.read_line(&mut line)?;
                if bytes == 0 {
                    return Ok(None);
                }
                self.line_no += 1;
                let trimmed = line.trim_end_matches(['\n', '\r']);
                if trimmed.trim().is_empty() {
                    continue;
                }
                return Ok(Some(RawRecord {
                    id: format!("{}:{}", self.source_path.display(), self.line_no),
                    source_path: self.source_path.clone(),
                    text: trimmed.to_string(),
                    meta: Default::default(),
                }));
            },
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
    fn newline_delimited_skips_blank_lines() {
        let f = write_temp("line1\n\nline2\n   \nline3\n");
        let mut src = PlainTextSource::open(f.path(), PlainTextMode::NewlineDelimited).unwrap();
        let mut texts = Vec::new();
        while let Some(r) = src.next_record().unwrap() {
            texts.push(r.text);
        }
        assert_eq!(texts, vec!["line1", "line2", "line3"]);
    }

    #[test]
    fn one_file_per_document_returns_single_record() {
        let f = write_temp("paragraph one.\nparagraph two.\n");
        let mut src = PlainTextSource::open(f.path(), PlainTextMode::OneFilePerDocument).unwrap();
        let first = src.next_record().unwrap().unwrap();
        assert!(first.text.contains("paragraph one"));
        assert!(first.text.contains("paragraph two"));
        assert!(src.next_record().unwrap().is_none());
    }

    #[test]
    fn empty_file_yields_no_records() {
        let f = write_temp("");
        let mut src = PlainTextSource::open(f.path(), PlainTextMode::NewlineDelimited).unwrap();
        assert!(src.next_record().unwrap().is_none());
    }
}
