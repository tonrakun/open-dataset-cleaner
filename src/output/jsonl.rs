use super::RecordSink;
use crate::record::{RawRecord, RejectionReason};
use crate::scoring::ScoreSet;
use serde::Serialize;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

#[derive(Serialize)]
struct AcceptedRecord<'a> {
    id: &'a str,
    text: &'a str,
    meta: &'a serde_json::Map<String, serde_json::Value>,
    scores: &'a ScoreSet,
}

#[derive(Serialize)]
struct RejectedRecord<'a> {
    id: &'a str,
    text: &'a str,
    meta: &'a serde_json::Map<String, serde_json::Value>,
    scores: &'a ScoreSet,
    reason: &'a RejectionReason,
}

pub struct JsonlSink {
    accepted_writer: BufWriter<File>,
    rejected_writer: Option<BufWriter<File>>,
}

impl JsonlSink {
    pub fn create(accepted_path: &Path, rejected_path: Option<&Path>) -> anyhow::Result<Self> {
        Self::open(accepted_path, rejected_path, false)
    }

    /// チェックポイントからの再開時に使う。`append=true`の場合、既存ファイルの末尾に
    /// 追記する(前回実行までに書き込まれた採用/除外レコードを保持したまま続きから書き込む)。
    pub fn create_or_append(
        accepted_path: &Path,
        rejected_path: Option<&Path>,
        append: bool,
    ) -> anyhow::Result<Self> {
        Self::open(accepted_path, rejected_path, append)
    }

    fn open(accepted_path: &Path, rejected_path: Option<&Path>, append: bool) -> anyhow::Result<Self> {
        if let Some(parent) = accepted_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let accepted_writer = BufWriter::new(open_file(accepted_path, append)?);
        let rejected_writer = match rejected_path {
            Some(p) => {
                if let Some(parent) = p.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                Some(BufWriter::new(open_file(p, append)?))
            }
            None => None,
        };
        Ok(Self { accepted_writer, rejected_writer })
    }
}

fn open_file(path: &Path, append: bool) -> std::io::Result<File> {
    if append {
        std::fs::OpenOptions::new().create(true).append(true).open(path)
    } else {
        File::create(path)
    }
}

impl RecordSink for JsonlSink {
    fn write_accepted(&mut self, record: &RawRecord, scores: &ScoreSet) -> anyhow::Result<()> {
        let out = AcceptedRecord {
            id: &record.id,
            text: &record.text,
            meta: &record.meta,
            scores,
        };
        serde_json::to_writer(&mut self.accepted_writer, &out)?;
        self.accepted_writer.write_all(b"\n")?;
        Ok(())
    }

    fn write_rejected(
        &mut self,
        record: &RawRecord,
        scores: &ScoreSet,
        reason: &RejectionReason,
    ) -> anyhow::Result<()> {
        if let Some(writer) = &mut self.rejected_writer {
            let out = RejectedRecord {
                id: &record.id,
                text: &record.text,
                meta: &record.meta,
                scores,
                reason,
            };
            serde_json::to_writer(&mut *writer, &out)?;
            writer.write_all(b"\n")?;
        }
        Ok(())
    }

    fn flush(&mut self) -> anyhow::Result<()> {
        self.accepted_writer.flush()?;
        if let Some(writer) = &mut self.rejected_writer {
            writer.flush()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead;

    fn sample_record() -> RawRecord {
        RawRecord {
            id: "id1".to_string(),
            source_path: "test.jsonl".into(),
            text: "hello".to_string(),
            meta: Default::default(),
        }
    }

    #[test]
    fn writes_accepted_and_rejected_to_separate_files() {
        let dir = tempfile::tempdir().unwrap();
        let accepted_path = dir.path().join("out.jsonl");
        let rejected_path = dir.path().join("out.rejected.jsonl");
        let mut sink = JsonlSink::create(&accepted_path, Some(&rejected_path)).unwrap();

        let record = sample_record();
        let scores = ScoreSet::default();
        sink.write_accepted(&record, &scores).unwrap();
        sink.write_rejected(&record, &scores, &RejectionReason::LanguageNotAllowed).unwrap();
        sink.flush().unwrap();

        let accepted_lines: Vec<String> = std::io::BufReader::new(File::open(&accepted_path).unwrap())
            .lines()
            .map(|l| l.unwrap())
            .collect();
        assert_eq!(accepted_lines.len(), 1);
        let parsed: serde_json::Value = serde_json::from_str(&accepted_lines[0]).unwrap();
        assert_eq!(parsed["id"], "id1");
        assert_eq!(parsed["text"], "hello");

        let rejected_lines: Vec<String> = std::io::BufReader::new(File::open(&rejected_path).unwrap())
            .lines()
            .map(|l| l.unwrap())
            .collect();
        assert_eq!(rejected_lines.len(), 1);
        let parsed: serde_json::Value = serde_json::from_str(&rejected_lines[0]).unwrap();
        assert_eq!(parsed["reason"], "language_not_allowed");
    }

    #[test]
    fn write_rejected_is_noop_without_rejected_writer() {
        let dir = tempfile::tempdir().unwrap();
        let accepted_path = dir.path().join("out.jsonl");
        let mut sink = JsonlSink::create(&accepted_path, None).unwrap();
        let record = sample_record();
        let scores = ScoreSet::default();
        sink.write_rejected(&record, &scores, &RejectionReason::ResidualUrlDetected).unwrap();
        sink.flush().unwrap();
    }
}
