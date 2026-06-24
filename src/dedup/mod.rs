pub mod exact;
pub mod minhash;

use crate::config::DedupConfig;
use crate::record::RejectionReason;
use exact::ExactDeduper;
use minhash::NearDuplicateDeduper;

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
    fn disabled_dedup_never_rejects() {
        let config = DedupConfig::default();
        let mut dedup = Deduplicator::from_config(&config);
        assert!(dedup.check_and_insert("same text").is_none());
        assert!(dedup.check_and_insert("same text").is_none());
    }
}
