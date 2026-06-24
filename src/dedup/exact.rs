use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// blake3ハッシュによる完全一致重複検出。本文そのものではなく32byteハッシュのみを保持する。
#[derive(Default)]
pub struct ExactDeduper {
    seen: HashSet<[u8; 32]>,
}

/// チェックポイント保存用のシリアライズ可能なスナップショット。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExactDeduperSnapshot {
    hashes: Vec<[u8; 32]>,
}

impl ExactDeduper {
    /// 未登録なら登録してtrueを返し、既に登録済みならfalseを返す。
    pub fn insert(&mut self, text: &str) -> bool {
        let hash = blake3::hash(text.as_bytes());
        self.seen.insert(*hash.as_bytes())
    }

    pub fn snapshot(&self) -> ExactDeduperSnapshot {
        ExactDeduperSnapshot { hashes: self.seen.iter().copied().collect() }
    }

    pub fn restore(snapshot: ExactDeduperSnapshot) -> Self {
        Self { seen: snapshot.hashes.into_iter().collect() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_exact_duplicate() {
        let mut d = ExactDeduper::default();
        assert!(d.insert("hello"));
        assert!(!d.insert("hello"));
        assert!(d.insert("world"));
    }
}
