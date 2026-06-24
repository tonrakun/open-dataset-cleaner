pub mod exact;
pub mod minhash;

use crate::config::DedupConfig;
use crate::record::RejectionReason;
use exact::{ExactDeduper, ExactDeduperSnapshot};
use minhash::{NearDuplicateDeduper, NearDuplicateDeduperSnapshot};
use serde::{Deserialize, Serialize};

/// 重複除去ステージ。スコアリング・閾値フィルタを通過したレコードに対し、
/// 完全一致 → 近似重複(MinHash/LSH)の順で重複判定を行う。
/// 状態(ハッシュ集合・LSHバケット)はレコード本文を保持せず要約値のみを保持するため、
/// メモリ使用量は採用済みレコード件数に比例し、本文サイズの総量には依存しない。
pub struct Deduplicator {
    exact: Option<ExactDeduper>,
    near: Option<NearDuplicateDeduper>,
}

impl Deduplicator {
    pub fn from_config(config: &DedupConfig) -> Self {
        Self {
            exact: config.exact.then(ExactDeduper::default),
            near: config.minhash_lsh.then(|| NearDuplicateDeduper::new(config)),
        }
    }

    /// 重複と判定された場合のみSomeを返す。重複でなければ内部状態に登録してNoneを返す。
    pub fn check_and_insert(&mut self, text: &str) -> Option<RejectionReason> {
        if let Some(exact) = self.exact.as_mut() {
            if !exact.insert(text) {
                return Some(RejectionReason::DuplicateExact);
            }
        }
        if let Some(near) = self.near.as_mut() {
            if near.check_and_insert(text).is_some() {
                return Some(RejectionReason::DuplicateNearDuplicate);
            }
        }
        None
    }

    /// チェックポイント保存用に内部状態を書き出す。
    pub fn snapshot(&self) -> DeduplicatorSnapshot {
        DeduplicatorSnapshot {
            exact: self.exact.as_ref().map(|e| e.snapshot()),
            near: self.near.as_ref().map(|n| n.snapshot()),
        }
    }

    /// `from_config` で構築済みのインスタンスに、チェックポイントから読み込んだ状態を復元する。
    /// 設定で無効化されているステージのスナップショットは無視される。
    pub fn restore(&mut self, snapshot: DeduplicatorSnapshot) {
        if let (Some(exact), Some(snap)) = (self.exact.as_mut(), snapshot.exact) {
            *exact = ExactDeduper::restore(snap);
        }
        if let (Some(near), Some(snap)) = (self.near.as_mut(), snapshot.near) {
            near.restore_into(snap);
        }
    }
}

/// チェックポイント保存用のシリアライズ可能なスナップショット。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeduplicatorSnapshot {
    exact: Option<ExactDeduperSnapshot>,
    near: Option<NearDuplicateDeduperSnapshot>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_dedup_rejects_second_identical_text() {
        let config = DedupConfig { exact: true, ..DedupConfig::default() };
        let mut dedup = Deduplicator::from_config(&config);
        assert!(dedup.check_and_insert("same text").is_none());
        assert!(matches!(dedup.check_and_insert("same text"), Some(RejectionReason::DuplicateExact)));
    }

    #[test]
    fn snapshot_and_restore_preserves_seen_records() {
        let config = DedupConfig { exact: true, minhash_lsh: true, ..DedupConfig::default() };
        let mut dedup = Deduplicator::from_config(&config);
        assert!(dedup.check_and_insert("first record text used for dedup snapshot test").is_none());
        let snapshot = dedup.snapshot();

        let mut restored = Deduplicator::from_config(&config);
        restored.restore(snapshot);
        assert!(
            matches!(
                restored.check_and_insert("first record text used for dedup snapshot test"),
                Some(RejectionReason::DuplicateExact)
            ),
            "復元後は同一テキストが重複として検出されるはず"
        );
        assert!(
            restored.check_and_insert("a completely different record about something else entirely").is_none(),
            "新規テキストはスナップショット復元後も重複と判定されないはず"
        );
    }

    #[test]
    fn disabled_dedup_never_rejects() {
        let config = DedupConfig::default();
        let mut dedup = Deduplicator::from_config(&config);
        assert!(dedup.check_and_insert("same text").is_none());
        assert!(dedup.check_and_insert("same text").is_none());
    }
}
