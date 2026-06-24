use super::{shard_path, RecordSink};
use crate::record::{RawRecord, RejectionReason};
use crate::scoring::ScoreSet;
use arrow::array::{ArrayRef, BooleanArray, Float64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// メモリを線形に増やさないよう、この行数に達するごとにRecordBatchを書き出す。
const BATCH_ROWS: usize = 10_000;

#[derive(Default)]
struct ScoreColumns {
    ids: Vec<String>,
    source_paths: Vec<String>,
    texts: Vec<String>,
    metas: Vec<String>,
    detected_languages: Vec<Option<String>>,
    language_mixed_ratios: Vec<f64>,
    char_hiragana: Vec<f64>,
    char_katakana: Vec<f64>,
    char_kanji: Vec<f64>,
    char_alnum: Vec<f64>,
    char_other: Vec<f64>,
    duplicate_line_ratios: Vec<f64>,
    symbol_digit_ratios: Vec<f64>,
    avg_sentence_lengths: Vec<f64>,
    sentence_length_variances: Vec<f64>,
    has_residual_html: Vec<bool>,
    has_residual_url: Vec<bool>,
}

impl ScoreColumns {
    fn len(&self) -> usize {
        self.ids.len()
    }

    fn push(&mut self, record: &RawRecord, scores: &ScoreSet) -> anyhow::Result<()> {
        self.ids.push(record.id.clone());
        self.source_paths.push(record.source_path.display().to_string());
        self.texts.push(record.text.clone());
        self.metas.push(serde_json::to_string(&record.meta)?);
        self.detected_languages.push(scores.detected_language.clone());
        self.language_mixed_ratios.push(scores.language_mixed_ratio);
        self.char_hiragana.push(scores.char_ratios.hiragana);
        self.char_katakana.push(scores.char_ratios.katakana);
        self.char_kanji.push(scores.char_ratios.kanji);
        self.char_alnum.push(scores.char_ratios.alnum);
        self.char_other.push(scores.char_ratios.other);
        self.duplicate_line_ratios.push(scores.duplicate_line_ratio);
        self.symbol_digit_ratios.push(scores.symbol_digit_ratio);
        self.avg_sentence_lengths.push(scores.avg_sentence_length);
        self.sentence_length_variances.push(scores.sentence_length_variance);
        self.has_residual_html.push(scores.has_residual_html);
        self.has_residual_url.push(scores.has_residual_url);
        Ok(())
    }

    fn take_arrays(&mut self) -> Vec<ArrayRef> {
        vec![
            Arc::new(StringArray::from_iter_values(std::mem::take(&mut self.ids))),
            Arc::new(StringArray::from_iter_values(std::mem::take(&mut self.source_paths))),
            Arc::new(StringArray::from_iter_values(std::mem::take(&mut self.texts))),
            Arc::new(StringArray::from_iter_values(std::mem::take(&mut self.metas))),
            Arc::new(StringArray::from(std::mem::take(&mut self.detected_languages))),
            Arc::new(Float64Array::from(std::mem::take(&mut self.language_mixed_ratios))),
            Arc::new(Float64Array::from(std::mem::take(&mut self.char_hiragana))),
            Arc::new(Float64Array::from(std::mem::take(&mut self.char_katakana))),
            Arc::new(Float64Array::from(std::mem::take(&mut self.char_kanji))),
            Arc::new(Float64Array::from(std::mem::take(&mut self.char_alnum))),
            Arc::new(Float64Array::from(std::mem::take(&mut self.char_other))),
            Arc::new(Float64Array::from(std::mem::take(&mut self.duplicate_line_ratios))),
            Arc::new(Float64Array::from(std::mem::take(&mut self.symbol_digit_ratios))),
            Arc::new(Float64Array::from(std::mem::take(&mut self.avg_sentence_lengths))),
            Arc::new(Float64Array::from(std::mem::take(&mut self.sentence_length_variances))),
            Arc::new(BooleanArray::from(std::mem::take(&mut self.has_residual_html))),
            Arc::new(BooleanArray::from(std::mem::take(&mut self.has_residual_url))),
        ]
    }
}

#[derive(Default)]
struct RejectedColumns {
    base: ScoreColumns,
    reasons: Vec<String>,
    details: Vec<Option<String>>,
}

impl RejectedColumns {
    fn len(&self) -> usize {
        self.base.len()
    }

    fn push(&mut self, record: &RawRecord, scores: &ScoreSet, reason: &RejectionReason) -> anyhow::Result<()> {
        self.base.push(record, scores)?;
        self.reasons.push(reason.as_key().to_string());
        self.details.push(match reason {
            RejectionReason::ExtractionError(message) => Some(message.clone()),
            RejectionReason::Plugin(detail) => Some(detail.clone()),
            _ => None,
        });
        Ok(())
    }

    fn take_arrays(&mut self) -> Vec<ArrayRef> {
        let mut arrays = self.base.take_arrays();
        arrays.push(Arc::new(StringArray::from_iter_values(std::mem::take(&mut self.reasons))));
        arrays.push(Arc::new(StringArray::from(std::mem::take(&mut self.details))));
        arrays
    }
}

fn score_fields() -> Vec<Field> {
    vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("source_path", DataType::Utf8, false),
        Field::new("text", DataType::Utf8, false),
        Field::new("meta", DataType::Utf8, false),
        Field::new("detected_language", DataType::Utf8, true),
        Field::new("language_mixed_ratio", DataType::Float64, false),
        Field::new("char_hiragana", DataType::Float64, false),
        Field::new("char_katakana", DataType::Float64, false),
        Field::new("char_kanji", DataType::Float64, false),
        Field::new("char_alnum", DataType::Float64, false),
        Field::new("char_other", DataType::Float64, false),
        Field::new("duplicate_line_ratio", DataType::Float64, false),
        Field::new("symbol_digit_ratio", DataType::Float64, false),
        Field::new("avg_sentence_length", DataType::Float64, false),
        Field::new("sentence_length_variance", DataType::Float64, false),
        Field::new("has_residual_html", DataType::Boolean, false),
        Field::new("has_residual_url", DataType::Boolean, false),
    ]
}

fn accepted_schema() -> Arc<Schema> {
    Arc::new(Schema::new(score_fields()))
}

fn rejected_schema() -> Arc<Schema> {
    let mut fields = score_fields();
    fields.push(Field::new("rejection_reason", DataType::Utf8, false));
    fields.push(Field::new("rejection_detail", DataType::Utf8, true));
    Arc::new(Schema::new(fields))
}

/// Parquet形式で受理/除外レコードを書き出す `RecordSink`。
/// `BATCH_ROWS`行ごとに `RecordBatch` を構築して `ArrowWriter::write` することで、
/// 全件をメモリに保持せずにストリーミングで書き出す。
pub struct ParquetSink {
    accepted_schema: Arc<Schema>,
    rejected_schema: Arc<Schema>,
    writer: Option<ArrowWriter<File>>,
    rejected_writer: Option<ArrowWriter<File>>,
    buffer: ScoreColumns,
    rejected_buffer: Option<RejectedColumns>,
    shard_max_rows: Option<u64>,
    accepted_base_path: PathBuf,
    rejected_base_path: Option<PathBuf>,
    accepted_shard_index: u64,
    rejected_shard_index: u64,
    accepted_rows_in_shard: u64,
    rejected_rows_in_shard: u64,
}

impl ParquetSink {
    pub fn create(accepted_path: &Path, rejected_path: Option<&Path>) -> anyhow::Result<Self> {
        Self::create_with_sharding(accepted_path, rejected_path, None)
    }

    /// `shard_max_rows`を指定すると、その行数に達するごとにfooterを閉じて新しい
    /// `<stem>.<5桁連番>.parquet`ファイルへ切り替える。Parquetはfooterを書くまで
    /// 有効なファイルにならないため、チェックポイント再開には対応しない
    /// (`pipeline::run`がチェックポイントをJSONL出力時のみ有効化している)。
    pub fn create_with_sharding(
        accepted_path: &Path,
        rejected_path: Option<&Path>,
        shard_max_rows: Option<u64>,
    ) -> anyhow::Result<Self> {
        if let Some(parent) = accepted_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let accepted_schema = accepted_schema();
        let first_accepted_path =
            if shard_max_rows.is_some() { shard_path(accepted_path, 0) } else { accepted_path.to_path_buf() };
        let file = File::create(&first_accepted_path)?;
        let writer = ArrowWriter::try_new(file, accepted_schema.clone(), None)?;

        let rejected_schema = rejected_schema();
        let (rejected_writer, rejected_buffer) = match rejected_path {
            Some(p) => {
                if let Some(parent) = p.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let first_rejected_path = if shard_max_rows.is_some() { shard_path(p, 0) } else { p.to_path_buf() };
                let rfile = File::create(&first_rejected_path)?;
                let rwriter = ArrowWriter::try_new(rfile, rejected_schema.clone(), None)?;
                (Some(rwriter), Some(RejectedColumns::default()))
            }
            None => (None, None),
        };

        Ok(Self {
            accepted_schema,
            rejected_schema,
            writer: Some(writer),
            rejected_writer,
            buffer: ScoreColumns::default(),
            rejected_buffer,
            shard_max_rows,
            accepted_base_path: accepted_path.to_path_buf(),
            rejected_base_path: rejected_path.map(|p| p.to_path_buf()),
            accepted_shard_index: 0,
            rejected_shard_index: 0,
            accepted_rows_in_shard: 0,
            rejected_rows_in_shard: 0,
        })
    }

    fn flush_accepted_batch(&mut self) -> anyhow::Result<()> {
        let batch_len = self.buffer.len();
        if batch_len == 0 {
            return Ok(());
        }
        let arrays = self.buffer.take_arrays();
        let batch = RecordBatch::try_new(self.accepted_schema.clone(), arrays)?;
        if let Some(writer) = self.writer.as_mut() {
            writer.write(&batch)?;
        }
        self.accepted_rows_in_shard += batch_len as u64;
        if let Some(max) = self.shard_max_rows {
            if max > 0 && self.accepted_rows_in_shard >= max {
                self.roll_accepted_shard()?;
            }
        }
        Ok(())
    }

    fn flush_rejected_batch(&mut self) -> anyhow::Result<()> {
        let Some(buffer) = self.rejected_buffer.as_mut() else {
            return Ok(());
        };
        let batch_len = buffer.len();
        if batch_len == 0 {
            return Ok(());
        }
        let arrays = buffer.take_arrays();
        let batch = RecordBatch::try_new(self.rejected_schema.clone(), arrays)?;
        if let Some(writer) = self.rejected_writer.as_mut() {
            writer.write(&batch)?;
        }
        self.rejected_rows_in_shard += batch_len as u64;
        if let Some(max) = self.shard_max_rows {
            if max > 0 && self.rejected_rows_in_shard >= max {
                self.roll_rejected_shard()?;
            }
        }
        Ok(())
    }

    fn roll_accepted_shard(&mut self) -> anyhow::Result<()> {
        if let Some(writer) = self.writer.take() {
            writer.close()?;
        }
        self.accepted_shard_index += 1;
        self.accepted_rows_in_shard = 0;
        let path = shard_path(&self.accepted_base_path, self.accepted_shard_index);
        let file = File::create(&path)?;
        self.writer = Some(ArrowWriter::try_new(file, self.accepted_schema.clone(), None)?);
        Ok(())
    }

    fn roll_rejected_shard(&mut self) -> anyhow::Result<()> {
        let Some(base_path) = self.rejected_base_path.as_ref() else {
            return Ok(());
        };
        if let Some(writer) = self.rejected_writer.take() {
            writer.close()?;
        }
        self.rejected_shard_index += 1;
        self.rejected_rows_in_shard = 0;
        let path = shard_path(base_path, self.rejected_shard_index);
        let file = File::create(&path)?;
        self.rejected_writer = Some(ArrowWriter::try_new(file, self.rejected_schema.clone(), None)?);
        Ok(())
    }
}

impl RecordSink for ParquetSink {
    fn write_accepted(&mut self, record: &RawRecord, scores: &ScoreSet) -> anyhow::Result<()> {
        self.buffer.push(record, scores)?;
        let should_flush = self.buffer.len() >= BATCH_ROWS
            || self
                .shard_max_rows
                .is_some_and(|max| self.accepted_rows_in_shard + self.buffer.len() as u64 >= max);
        if should_flush {
            self.flush_accepted_batch()?;
        }
        Ok(())
    }

    fn write_rejected(
        &mut self,
        record: &RawRecord,
        scores: &ScoreSet,
        reason: &RejectionReason,
    ) -> anyhow::Result<()> {
        if self.rejected_buffer.is_none() {
            return Ok(());
        }
        self.rejected_buffer.as_mut().unwrap().push(record, scores, reason)?;
        let rejected_len = self.rejected_buffer.as_ref().map(|b| b.len()).unwrap_or(0);
        let should_flush = rejected_len >= BATCH_ROWS
            || self
                .shard_max_rows
                .is_some_and(|max| self.rejected_rows_in_shard + rejected_len as u64 >= max);
        if should_flush {
            self.flush_rejected_batch()?;
        }
        Ok(())
    }

    fn flush(&mut self) -> anyhow::Result<()> {
        self.flush_accepted_batch()?;
        self.flush_rejected_batch()?;
        if let Some(writer) = self.writer.take() {
            writer.close()?;
        }
        if let Some(writer) = self.rejected_writer.take() {
            writer.close()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

    fn sample_record(id: &str, text: &str) -> RawRecord {
        RawRecord {
            id: id.to_string(),
            source_path: "test.html".into(),
            text: text.to_string(),
            meta: Default::default(),
        }
    }

    fn read_all_rows(path: &Path) -> Vec<RecordBatch> {
        let file = File::open(path).unwrap();
        let reader = ParquetRecordBatchReaderBuilder::try_new(file).unwrap().build().unwrap();
        reader.map(|b| b.unwrap()).collect()
    }

    #[test]
    fn writes_accepted_and_rejected_parquet_files() {
        let dir = tempfile::tempdir().unwrap();
        let accepted_path = dir.path().join("out.parquet");
        let rejected_path = dir.path().join("out.rejected.parquet");
        let mut sink = ParquetSink::create(&accepted_path, Some(&rejected_path)).unwrap();

        let scores = ScoreSet::default();
        sink.write_accepted(&sample_record("id1", "hello"), &scores).unwrap();
        sink.write_rejected(&sample_record("id2", "world"), &scores, &RejectionReason::LanguageNotAllowed)
            .unwrap();
        sink.flush().unwrap();

        let accepted_batches = read_all_rows(&accepted_path);
        let total_accepted: usize = accepted_batches.iter().map(|b| b.num_rows()).sum();
        assert_eq!(total_accepted, 1);
        let id_col = accepted_batches[0].column_by_name("id").unwrap().as_any().downcast_ref::<StringArray>().unwrap();
        assert_eq!(id_col.value(0), "id1");

        let rejected_batches = read_all_rows(&rejected_path);
        let total_rejected: usize = rejected_batches.iter().map(|b| b.num_rows()).sum();
        assert_eq!(total_rejected, 1);
        let reason_col = rejected_batches[0]
            .column_by_name("rejection_reason")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(reason_col.value(0), "language_not_allowed");
    }

    #[test]
    fn write_rejected_is_noop_without_rejected_writer() {
        let dir = tempfile::tempdir().unwrap();
        let accepted_path = dir.path().join("out.parquet");
        let mut sink = ParquetSink::create(&accepted_path, None).unwrap();
        let scores = ScoreSet::default();
        sink.write_rejected(&sample_record("id1", "x"), &scores, &RejectionReason::ResidualUrlDetected)
            .unwrap();
        sink.flush().unwrap();
    }

    #[test]
    fn splits_output_into_shards_when_max_rows_configured() {
        let dir = tempfile::tempdir().unwrap();
        let accepted_path = dir.path().join("out.parquet");
        let mut sink = ParquetSink::create_with_sharding(&accepted_path, None, Some(2)).unwrap();

        let scores = ScoreSet::default();
        for i in 0..5 {
            sink.write_accepted(&sample_record(&format!("id{i}"), "hello"), &scores).unwrap();
        }
        sink.flush().unwrap();

        assert!(!accepted_path.exists());
        let shard0 = dir.path().join("out.00000.parquet");
        let shard1 = dir.path().join("out.00001.parquet");
        let shard2 = dir.path().join("out.00002.parquet");
        let rows = |p: &Path| read_all_rows(p).iter().map(|b| b.num_rows()).sum::<usize>();
        assert_eq!(rows(&shard0), 2);
        assert_eq!(rows(&shard1), 2);
        assert_eq!(rows(&shard2), 1);
    }
}
