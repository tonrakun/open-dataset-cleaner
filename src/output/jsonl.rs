use super::{shard_path, RecordSink};
use crate::record::{RawRecord, RejectionReason};
use crate::scoring::ScoreSet;
use serde::Serialize;
use std::fs::File;
use std::io::{BufRead, BufWriter, Write};
use std::path::{Path, PathBuf};

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

/// `max_rows`を超えないよう、しきい値に達するごとに新しい連番ファイルへ
/// 切り替えて書き込む行単位ライター。`max_rows=None`の場合は`base_path`に
/// 単一ファイルとして書き込む(シャード分割なし、既存の名前を保つ)。
struct ShardedWriter {
    base_path: PathBuf,
    max_rows: Option<u64>,
    shard_index: u64,
    rows_in_shard: u64,
    writer: BufWriter<File>,
}

impl ShardedWriter {
    fn create(base_path: &Path, max_rows: Option<u64>, append: bool) -> anyhow::Result<Self> {
        if let Some(parent) = base_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let (shard_index, rows_in_shard) = if append && max_rows.is_some() {
            Self::resume_last_shard(base_path)?
        } else {
            (0, 0)
        };
        let path = if max_rows.is_some() { shard_path(base_path, shard_index) } else { base_path.to_path_buf() };
        let writer = BufWriter::new(open_file(&path, append)?);
        Ok(Self { base_path: base_path.to_path_buf(), max_rows, shard_index, rows_in_shard, writer })
    }

    /// チェックポイント再開時、既存のシャードファイルを連番で走査し、最後のシャードの
    /// 行数を数えて続きから書き込めるようにする(シャード自体は跨いで数えない)。
    fn resume_last_shard(base_path: &Path) -> anyhow::Result<(u64, u64)> {
        let mut index = 0u64;
        while shard_path(base_path, index).exists() {
            index += 1;
        }
        if index == 0 {
            return Ok((0, 0));
        }
        let last_index = index - 1;
        let rows = count_lines(&shard_path(base_path, last_index))?;
        Ok((last_index, rows))
    }

    fn write_line(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        if let Some(max) = self.max_rows {
            if max > 0 && self.rows_in_shard >= max {
                self.roll_to_next_shard()?;
            }
        }
        self.writer.write_all(bytes)?;
        self.writer.write_all(b"\n")?;
        self.rows_in_shard += 1;
        Ok(())
    }

    fn roll_to_next_shard(&mut self) -> anyhow::Result<()> {
        self.writer.flush()?;
        self.shard_index += 1;
        self.rows_in_shard = 0;
        let path = shard_path(&self.base_path, self.shard_index);
        self.writer = BufWriter::new(open_file(&path, false)?);
        Ok(())
    }

    fn flush(&mut self) -> anyhow::Result<()> {
        self.writer.flush().map_err(Into::into)
    }
}

fn count_lines(path: &Path) -> anyhow::Result<u64> {
    let file = File::open(path)?;
    Ok(std::io::BufReader::new(file).lines().count() as u64)
}

pub struct JsonlSink {
    accepted: ShardedWriter,
    rejected: Option<ShardedWriter>,
}

impl JsonlSink {
    pub fn create(accepted_path: &Path, rejected_path: Option<&Path>) -> anyhow::Result<Self> {
        Self::open(accepted_path, rejected_path, false, None)
    }

    /// チェックポイントからの再開時に使う。`append=true`の場合、既存ファイルの末尾に
    /// 追記する(前回実行までに書き込まれた採用/除外レコードを保持したまま続きから書き込む)。
    pub fn create_or_append(
        accepted_path: &Path,
        rejected_path: Option<&Path>,
        append: bool,
    ) -> anyhow::Result<Self> {
        Self::open(accepted_path, rejected_path, append, None)
    }

    pub fn create_with_sharding(
        accepted_path: &Path,
        rejected_path: Option<&Path>,
        append: bool,
        shard_max_rows: Option<u64>,
    ) -> anyhow::Result<Self> {
        Self::open(accepted_path, rejected_path, append, shard_max_rows)
    }

    fn open(
        accepted_path: &Path,
        rejected_path: Option<&Path>,
        append: bool,
        shard_max_rows: Option<u64>,
    ) -> anyhow::Result<Self> {
        let accepted = ShardedWriter::create(accepted_path, shard_max_rows, append)?;
        let rejected = match rejected_path {
            Some(p) => Some(ShardedWriter::create(p, shard_max_rows, append)?),
            None => None,
        };
        Ok(Self { accepted, rejected })
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
        let line = serde_json::to_vec(&out)?;
        self.accepted.write_line(&line)
    }

    fn write_rejected(
        &mut self,
        record: &RawRecord,
        scores: &ScoreSet,
        reason: &RejectionReason,
    ) -> anyhow::Result<()> {
        if let Some(writer) = &mut self.rejected {
            let out = RejectedRecord {
                id: &record.id,
                text: &record.text,
                meta: &record.meta,
                scores,
                reason,
            };
            let line = serde_json::to_vec(&out)?;
            writer.write_line(&line)?;
        }
        Ok(())
    }

    fn flush(&mut self) -> anyhow::Result<()> {
        self.accepted.flush()?;
        if let Some(writer) = &mut self.rejected {
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

    #[test]
    fn splits_output_into_shards_when_max_rows_configured() {
        let dir = tempfile::tempdir().unwrap();
        let accepted_path = dir.path().join("out.jsonl");
        let mut sink =
            JsonlSink::create_with_sharding(&accepted_path, None, false, Some(2)).unwrap();

        let scores = ScoreSet::default();
        for i in 0..5 {
            let mut record = sample_record();
            record.id = format!("id{i}");
            sink.write_accepted(&record, &scores).unwrap();
        }
        sink.flush().unwrap();

        let shard0 = dir.path().join("out.00000.jsonl");
        let shard1 = dir.path().join("out.00001.jsonl");
        let shard2 = dir.path().join("out.00002.jsonl");
        assert!(!accepted_path.exists());
        let count_lines = |p: &Path| std::io::BufReader::new(File::open(p).unwrap()).lines().count();
        assert_eq!(count_lines(&shard0), 2);
        assert_eq!(count_lines(&shard1), 2);
        assert_eq!(count_lines(&shard2), 1);
    }

    #[test]
    fn resumes_into_last_shard_and_continues_counting_rows() {
        let dir = tempfile::tempdir().unwrap();
        let accepted_path = dir.path().join("out.jsonl");
        {
            let mut sink =
                JsonlSink::create_with_sharding(&accepted_path, None, false, Some(2)).unwrap();
            let scores = ScoreSet::default();
            for i in 0..3 {
                let mut record = sample_record();
                record.id = format!("id{i}");
                sink.write_accepted(&record, &scores).unwrap();
            }
            sink.flush().unwrap();
        }
        // out.00000.jsonl(2行) + out.00001.jsonl(1行)が存在する状態から再開する。
        {
            let mut sink =
                JsonlSink::create_with_sharding(&accepted_path, None, true, Some(2)).unwrap();
            let scores = ScoreSet::default();
            let mut record = sample_record();
            record.id = "id3".to_string();
            sink.write_accepted(&record, &scores).unwrap();
            sink.flush().unwrap();
        }

        let shard1 = dir.path().join("out.00001.jsonl");
        let lines: Vec<String> =
            std::io::BufReader::new(File::open(&shard1).unwrap()).lines().map(|l| l.unwrap()).collect();
        assert_eq!(lines.len(), 2);
        assert!(!dir.path().join("out.00002.jsonl").exists());
    }
}
