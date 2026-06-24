use crate::config::DedupConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const MERSENNE_PRIME: u64 = (1u64 << 61) - 1;

fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let mut hash = OFFSET_BASIS;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// テキストから文字単位のn-gram指紋を生成する。単語分割に依存しないため、
/// 空白で単語分割できない日本語などの文章にも同じロジックで対応できる。
pub fn char_shingles(text: &str, size: usize) -> Vec<u64> {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return Vec::new();
    }
    if chars.len() < size {
        return vec![fnv1a64(text.as_bytes())];
    }
    chars
        .windows(size)
        .map(|w| {
            let s: String = w.iter().collect();
            fnv1a64(s.as_bytes())
        })
        .collect()
}

/// 固定シードのxorshift64による決定的な疑似乱数生成器。
/// 標準のOS乱数を使わないのは、同一設定での再実行時にハッシュ関数の係数が
/// 変動せず同一の重複判定結果になることを保証するため（要件: 処理の冪等性）。
struct DeterministicRng(u64);

impl DeterministicRng {
    fn new(seed: u64) -> Self {
        Self(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

/// MinHashシグネチャ生成 + LSHバンディングによる近似重複検出。
/// 本文ではなくシグネチャ(u64配列)とバケット中のレコードIDのみを保持するため、
/// メモリ使用量は採用済みレコード件数 × num_hashesに比例し、本文サイズには依存しない。
pub struct NearDuplicateDeduper {
    num_hashes: usize,
    num_bands: usize,
    rows_per_band: usize,
    shingle_size: usize,
    similarity_threshold: f64,
    coeffs: Vec<(u64, u64)>,
    signatures: Vec<Vec<u64>>,
    buckets: HashMap<(usize, u64), Vec<usize>>,
}

impl NearDuplicateDeduper {
    pub fn new(config: &DedupConfig) -> Self {
        let num_hashes = config.num_hashes.max(1);
        let num_bands = config.num_bands.max(1);
        let rows_per_band = (num_hashes / num_bands).max(1);
        let mut rng = DeterministicRng::new(0x2545_F491_4F6C_DD1D);
        let coeffs = (0..num_hashes)
            .map(|_| {
                let a = (rng.next() % (MERSENNE_PRIME - 1)) + 1;
                let b = rng.next() % MERSENNE_PRIME;
                (a, b)
            })
            .collect();
        Self {
            num_hashes,
            num_bands,
            rows_per_band,
            shingle_size: config.shingle_size.max(1),
            similarity_threshold: config.similarity_threshold,
            coeffs,
            signatures: Vec::new(),
            buckets: HashMap::new(),
        }
    }

    fn signature(&self, text: &str) -> Vec<u64> {
        let shingles = char_shingles(text, self.shingle_size);
        if shingles.is_empty() {
            return vec![0; self.num_hashes];
        }
        self.coeffs
            .iter()
            .map(|&(a, b)| {
                shingles
                    .iter()
                    .map(|&x| ((a as u128 * x as u128 + b as u128) % MERSENNE_PRIME as u128) as u64)
                    .min()
                    .unwrap()
            })
            .collect()
    }

    fn band_hash(&self, signature: &[u64], band: usize) -> u64 {
        let start = band * self.rows_per_band;
        let end = (start + self.rows_per_band).min(signature.len());
        let mut bytes = Vec::with_capacity((end - start) * 8);
        for v in &signature[start..end] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        fnv1a64(&bytes)
    }

    fn estimated_similarity(&self, a: &[u64], b: &[u64]) -> f64 {
        let matches = a.iter().zip(b.iter()).filter(|(x, y)| x == y).count();
        matches as f64 / self.num_hashes as f64
    }

    /// 近似重複候補がなければシグネチャを登録してNoneを返す。
    /// 類似度がしきい値以上の既存レコードIDが見つかればSomeを返す(その場合は登録しない)。
    pub fn check_and_insert(&mut self, text: &str) -> Option<usize> {
        let signature = self.signature(text);
        let mut candidate_ids: Vec<usize> = Vec::new();
        for band in 0..self.num_bands {
            let key = (band, self.band_hash(&signature, band));
            if let Some(ids) = self.buckets.get(&key) {
                candidate_ids.extend(ids.iter().copied());
            }
        }
        candidate_ids.sort_unstable();
        candidate_ids.dedup();
        for id in candidate_ids {
            if self.estimated_similarity(&signature, &self.signatures[id]) >= self.similarity_threshold {
                return Some(id);
            }
        }

        let id = self.signatures.len();
        for band in 0..self.num_bands {
            let key = (band, self.band_hash(&signature, band));
            self.buckets.entry(key).or_default().push(id);
        }
        self.signatures.push(signature);
        None
    }

    /// チェックポイント保存用に、登録済みシグネチャ・LSHバケットを書き出す。
    /// `coeffs`は設定から決定的に再生成されるため保存不要。
    pub fn snapshot(&self) -> NearDuplicateDeduperSnapshot {
        NearDuplicateDeduperSnapshot {
            signatures: self.signatures.clone(),
            buckets: self.buckets.iter().map(|(k, v)| (*k, v.clone())).collect(),
        }
    }

    /// `new(config)` で構築済みのインスタンス(係数は設定から再生成される)に、
    /// チェックポイントから読み込んだシグネチャ・バケットを復元する。
    pub fn restore_into(&mut self, snapshot: NearDuplicateDeduperSnapshot) {
        self.signatures = snapshot.signatures;
        self.buckets = snapshot.buckets.into_iter().collect();
    }
}

/// チェックポイント保存用のシリアライズ可能なスナップショット。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NearDuplicateDeduperSnapshot {
    signatures: Vec<Vec<u64>>,
    buckets: Vec<((usize, u64), Vec<usize>)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> DedupConfig {
        DedupConfig {
            exact: false,
            minhash_lsh: true,
            similarity_threshold: 0.6,
            num_hashes: 64,
            num_bands: 16,
            shingle_size: 8,
        }
    }

    #[test]
    fn detects_near_duplicate_with_minor_edits() {
        let mut d = NearDuplicateDeduper::new(&config());
        let original = "the quick brown fox jumps over the lazy dog near the river every single morning";
        let near_dup = "the quick brown fox jumps over the lazy dog near the river every single evening";
        let different = "completely unrelated content about cooking pasta with tomatoes and fresh basil leaves";

        assert!(d.check_and_insert(original).is_none());
        assert!(d.check_and_insert(near_dup).is_some(), "わずかな編集はLSHで重複候補として検出されるはず");
        assert!(d.check_and_insert(different).is_none(), "全く異なる文章は重複と判定されないはず");
    }

    #[test]
    fn shingles_are_deterministic() {
        let a = char_shingles("hello world", 5);
        let b = char_shingles("hello world", 5);
        assert_eq!(a, b);
        assert!(!a.is_empty());
    }
}
