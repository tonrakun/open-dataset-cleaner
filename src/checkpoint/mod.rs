//! 途中再開対応(チェックポイント)。
//!
//! 処理済み入力ファイルの一覧と、重複除去・統計の内部状態を
//! `runtime.checkpoint_dir` 配下の `checkpoint.json` に保存する。
//! 再実行時にこのファイルを読み込み、設定が前回と一致していれば
//! 処理済みファイルをスキップして続きから再開する。
//!
//! 出力フォーマットがJSONLの場合のみ対応する(`pipeline::run`側でガードする)。
//! Parquetはファイルを閉じるまで有効なファイルにならず、安全に追記再開できないため。

use crate::config::Config;
use crate::dedup::DeduplicatorSnapshot;
use crate::stats::StatsAccumulatorSnapshot;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const CHECKPOINT_FILE_NAME: &str = "checkpoint.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointState {
    pub config_fingerprint: String,
    pub completed_files: BTreeSet<String>,
    pub dedup: DeduplicatorSnapshot,
    pub stats: StatsAccumulatorSnapshot,
}

/// 出力に影響する設定だけからフィンガープリントを計算する。
/// `input.paths`は意図的に除外する(再開時に入力ファイルが増えるのは想定動作のため)。
/// `runtime`・`stats.output_path`等の運用パラメータも出力内容に影響しないため除外する。
pub fn fingerprint(config: &Config) -> String {
    let mut input = config.input.clone();
    input.paths = Vec::new();
    let parts = format!(
        "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
        input, config.extract, config.scoring, config.filters, config.dedup, config.plugins,
    );
    blake3::hash(parts.as_bytes()).to_hex().to_string()
}

fn checkpoint_path(dir: &Path) -> PathBuf {
    dir.join(CHECKPOINT_FILE_NAME)
}

pub fn load(dir: &Path) -> anyhow::Result<Option<CheckpointState>> {
    let path = checkpoint_path(dir);
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("チェックポイント {} の読み込みに失敗: {}", path.display(), e))?;
    let state: CheckpointState = serde_json::from_str(&raw)
        .map_err(|e| anyhow::anyhow!("チェックポイント {} の解析に失敗: {}", path.display(), e))?;
    Ok(Some(state))
}

/// 書き込み中のプロセスクラッシュで破損したチェックポイントを読み込まないよう、
/// 一時ファイルに書き出してからリネームすることでアトミックに更新する。
pub fn save(dir: &Path, state: &CheckpointState) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = checkpoint_path(dir);
    let tmp_path = dir.join(format!("{}.tmp", CHECKPOINT_FILE_NAME));
    let json = serde_json::to_string(state)?;
    std::fs::write(&tmp_path, json)?;
    std::fs::rename(&tmp_path, &path)?;
    Ok(())
}

/// 入力ファイルをチェックポイント上で一意に識別するためのキー。
/// 実行ディレクトリが変わっても同一ファイルを指し示せるよう正規化する。
pub fn file_key(path: &Path) -> String {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()).to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dedup::Deduplicator;
    use crate::stats::StatsAccumulator;

    #[test]
    fn save_and_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let mut completed_files = BTreeSet::new();
        completed_files.insert("a.jsonl".to_string());
        let state = CheckpointState {
            config_fingerprint: "abc".to_string(),
            completed_files,
            dedup: Deduplicator::from_config(&Config::default().dedup).snapshot(),
            stats: StatsAccumulator::default().snapshot(),
        };
        save(dir.path(), &state).unwrap();

        let loaded = load(dir.path()).unwrap().unwrap();
        assert_eq!(loaded.config_fingerprint, "abc");
        assert!(loaded.completed_files.contains("a.jsonl"));
    }

    #[test]
    fn load_returns_none_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load(dir.path()).unwrap().is_none());
    }

    #[test]
    fn fingerprint_is_stable_for_identical_config_and_changes_with_scoring() {
        let mut a = Config::default();
        a.input.paths = vec!["./a/*.jsonl".to_string()];
        let mut b = Config::default();
        b.input.paths = vec!["./b/*.jsonl".to_string()];
        assert_eq!(fingerprint(&a), fingerprint(&b), "input.pathsはフィンガープリントに影響しないはず");

        let mut c = Config::default();
        c.scoring.language.allow = vec!["ja".to_string()];
        assert_ne!(fingerprint(&a), fingerprint(&c), "scoringの変更はフィンガープリントに反映されるはず");
    }
}
