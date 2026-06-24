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

/// シャード分割時のファイル名を`<stem>.<5桁連番>.<拡張子>`で生成する。
/// 例: `./out/dataset.jsonl` のシャード1番目 -> `./out/dataset.00001.jsonl`
pub(crate) fn shard_path(base: &std::path::Path, index: u64) -> std::path::PathBuf {
    let stem = base.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let name = match base.extension() {
        Some(ext) => format!("{stem}.{index:05}.{}", ext.to_string_lossy()),
        None => format!("{stem}.{index:05}"),
    };
    base.with_file_name(name)
}

#[cfg(test)]
mod shard_path_tests {
    use super::shard_path;
    use std::path::Path;

    #[test]
    fn inserts_zero_padded_index_before_extension() {
        let path = shard_path(Path::new("./out/dataset.jsonl"), 1);
        assert_eq!(path, Path::new("./out/dataset.00001.jsonl"));
    }

    #[test]
    fn handles_paths_without_extension() {
        let path = shard_path(Path::new("./out/dataset"), 0);
        assert_eq!(path, Path::new("./out/dataset.00000"));
    }
}
