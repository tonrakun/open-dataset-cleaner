pub mod jsonl;
pub mod parquet;

pub use jsonl::JsonlSink;
pub use parquet::ParquetSink;

use crate::record::{RawRecord, RejectionReason};
use crate::scoring::ScoreSet;

pub trait RecordSink: Send {
    fn write_accepted(&mut self, record: &RawRecord, scores: &ScoreSet) -> anyhow::Result<()>;
    fn write_rejected(
        &mut self,
        record: &RawRecord,
        scores: &ScoreSet,
        reason: &RejectionReason,
    ) -> anyhow::Result<()>;
    fn flush(&mut self) -> anyhow::Result<()>;
}
