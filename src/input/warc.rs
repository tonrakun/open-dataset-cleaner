use super::RecordSource;
use crate::record::RawRecord;
use flate2::read::MultiGzDecoder;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

/// WARC (`.warc` / `.warc.gz`) を自前パーサで読み込む `RecordSource`。
/// `WARC-Type: response` のレコードのみを採用し、HTTPレスポンスから本文(生HTML)を取り出す。
pub struct WarcSource {
    source_path: PathBuf,
    reader: Box<dyn BufRead + Send>,
    record_no: u64,
}

impl WarcSource {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let file = File::open(path)
            .map_err(|e| anyhow::anyhow!("ファイルを開けません {}: {}", path.display(), e))?;
        let is_gzip = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.eq_ignore_ascii_case("gz"))
            .unwrap_or(false);
        let reader: Box<dyn BufRead + Send> = if is_gzip {
            Box::new(BufReader::new(MultiGzDecoder::new(file)))
        } else {
            Box::new(BufReader::new(file))
        };
        Ok(Self {
            source_path: path.to_path_buf(),
            reader,
            record_no: 0,
        })
    }

    /// 1レコード分のヘッダとブロック本文を読む。EOFはNoneで返す。
    fn read_raw_record(&mut self) -> anyhow::Result<Option<(HashMap<String, String>, Vec<u8>)>> {
        let mut line = String::new();
        loop {
            line.clear();
            let bytes = self.reader.read_line(&mut line)?;
            if bytes == 0 {
                return Ok(None);
            }
            if line.starts_with("WARC/") {
                break;
            }
            // 空行・前レコードの末尾区切りはスキップして次行を探す。
        }

        let mut headers = HashMap::new();
        loop {
            line.clear();
            let bytes = self.reader.read_line(&mut line)?;
            if bytes == 0 {
                anyhow::bail!(
                    "{}: WARCレコードが不完全です（ヘッダ終端前にEOF）",
                    self.source_path.display()
                );
            }
            let trimmed = line.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() {
                break;
            }
            if let Some((key, value)) = trimmed.split_once(':') {
                headers.insert(key.trim().to_string(), value.trim().to_string());
            }
        }

        let content_length: usize = headers
            .get("Content-Length")
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0);
        let mut body = vec![0u8; content_length];
        self.reader
            .read_exact(&mut body)
            .map_err(|e| anyhow::anyhow!("{}: WARCブロック本文の読み込みに失敗: {}", self.source_path.display(), e))?;

        Ok(Some((headers, body)))
    }
}

impl RecordSource for WarcSource {
    fn next_record(&mut self) -> anyhow::Result<Option<RawRecord>> {
        loop {
            let (headers, body) = match self.read_raw_record()? {
                Some(v) => v,
                None => return Ok(None),
            };

            if headers.get("WARC-Type").map(String::as_str) != Some("response") {
                continue;
            }
            self.record_no += 1;

            let html_body = split_http_body(&body);
            let text = String::from_utf8_lossy(html_body).into_owned();
            if text.trim().is_empty() {
                continue;
            }

            let url = headers.get("WARC-Target-URI").cloned();
            let mut meta = serde_json::Map::new();
            if let Some(u) = &url {
                meta.insert("url".to_string(), serde_json::Value::String(u.clone()));
            }

            let id = url.unwrap_or_else(|| format!("{}:{}", self.source_path.display(), self.record_no));
            return Ok(Some(RawRecord {
                id,
                source_path: self.source_path.clone(),
                text,
                meta,
            }));
        }
    }
}

/// HTTPレスポンスのバイト列からヘッダ部を除いた本文を返す。区切りが見つからない場合は全体を返す。
fn split_http_body(raw: &[u8]) -> &[u8] {
    if let Some(pos) = find_subslice(raw, b"\r\n\r\n") {
        &raw[pos + 4..]
    } else if let Some(pos) = find_subslice(raw, b"\n\n") {
        &raw[pos + 2..]
    } else {
        raw
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    fn sample_warc_bytes() -> Vec<u8> {
        let http_response = b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n<html><body>hello</body></html>";
        let mut record = String::new();
        record.push_str("WARC/1.0\r\n");
        record.push_str("WARC-Type: response\r\n");
        record.push_str("WARC-Target-URI: http://example.com/\r\n");
        record.push_str(&format!("Content-Length: {}\r\n", http_response.len()));
        record.push_str("\r\n");
        let mut bytes = record.into_bytes();
        bytes.extend_from_slice(http_response);
        bytes.extend_from_slice(b"\r\n\r\n");

        // warcinfo レコード（response以外はスキップされることを確認するため先頭に挿入）
        let info_body = b"format: WARC File Format 1.0";
        let mut info = String::new();
        info.push_str("WARC/1.0\r\n");
        info.push_str("WARC-Type: warcinfo\r\n");
        info.push_str(&format!("Content-Length: {}\r\n", info_body.len()));
        info.push_str("\r\n");
        let mut all = info.into_bytes();
        all.extend_from_slice(info_body);
        all.extend_from_slice(b"\r\n\r\n");
        all.extend_from_slice(&bytes);
        all
    }

    fn write_temp(bytes: &[u8], suffix: &str) -> tempfile::TempPath {
        let mut f = tempfile::Builder::new().suffix(suffix).tempfile().unwrap();
        f.write_all(bytes).unwrap();
        f.into_temp_path()
    }

    #[test]
    fn reads_response_record_and_skips_others() {
        let bytes = sample_warc_bytes();
        let path = write_temp(&bytes, ".warc");
        let mut src = WarcSource::open(&path).unwrap();
        let record = src.next_record().unwrap().unwrap();
        assert_eq!(record.text, "<html><body>hello</body></html>");
        assert_eq!(record.meta.get("url").unwrap(), "http://example.com/");
        assert!(src.next_record().unwrap().is_none());
    }

    #[test]
    fn reads_gzip_compressed_warc() {
        let bytes = sample_warc_bytes();
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&bytes).unwrap();
        let gz_bytes = encoder.finish().unwrap();

        let path = write_temp(&gz_bytes, ".warc.gz");
        let mut src = WarcSource::open(&path).unwrap();
        let record = src.next_record().unwrap().unwrap();
        assert_eq!(record.text, "<html><body>hello</body></html>");
        assert!(src.next_record().unwrap().is_none());
    }
}
