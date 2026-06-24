use serde::Deserialize;

fn default_similarity_threshold() -> f64 {
    0.85
}

fn default_num_hashes() -> usize {
    128
}

fn default_num_bands() -> usize {
    16
}

fn default_shingle_size() -> usize {
    25
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct DedupConfig {
    #[serde(default)]
    pub exact: bool,
    #[serde(default)]
    pub minhash_lsh: bool,
    #[serde(default = "default_similarity_threshold")]
    pub similarity_threshold: f64,
    #[serde(default = "default_num_hashes")]
    pub num_hashes: usize,
    #[serde(default = "default_num_bands")]
    pub num_bands: usize,
    #[serde(default = "default_shingle_size")]
    pub shingle_size: usize,
}

impl Default for DedupConfig {
    fn default() -> Self {
        Self {
            exact: false,
            minhash_lsh: false,
            similarity_threshold: default_similarity_threshold(),
            num_hashes: default_num_hashes(),
            num_bands: default_num_bands(),
            shingle_size: default_shingle_size(),
        }
    }
}

impl DedupConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.minhash_lsh {
            if self.num_bands == 0 || !self.num_hashes.is_multiple_of(self.num_bands) {
                anyhow::bail!(
                    "dedup.num_hashes({})はdedup.num_bands({})で割り切れる必要があります",
                    self.num_hashes,
                    self.num_bands
                );
            }
            if !(0.0..=1.0).contains(&self.similarity_threshold) {
                anyhow::bail!("dedup.similarity_thresholdは0.0〜1.0で指定してください");
            }
        }
        Ok(())
    }
}
