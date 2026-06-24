use serde::Serialize;

/// Welfordのオンラインアルゴリズムによる並列安全な統計集計（mean/variance/min/max）。
#[derive(Debug, Clone)]
pub struct RunningStats {
    pub count: u64,
    pub mean: f64,
    pub m2: f64,
    pub min: f64,
    pub max: f64,
}

impl Default for RunningStats {
    fn default() -> Self {
        Self {
            count: 0,
            mean: 0.0,
            m2: 0.0,
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
        }
    }
}

impl RunningStats {
    pub fn push(&mut self, value: f64) {
        self.count += 1;
        let delta = value - self.mean;
        self.mean += delta / self.count as f64;
        let delta2 = value - self.mean;
        self.m2 += delta * delta2;
        self.min = self.min.min(value);
        self.max = self.max.max(value);
    }

    pub fn variance(&self) -> f64 {
        if self.count < 2 {
            0.0
        } else {
            self.m2 / self.count as f64
        }
    }

    /// Chanらの並列分散結合アルゴリズムでバッチごとの集計を合算する。
    pub fn merge(&self, other: &RunningStats) -> RunningStats {
        if self.count == 0 {
            return other.clone();
        }
        if other.count == 0 {
            return self.clone();
        }
        let count = self.count + other.count;
        let delta = other.mean - self.mean;
        let mean = self.mean + delta * (other.count as f64 / count as f64);
        let m2 = self.m2 + other.m2 + delta * delta * (self.count as f64 * other.count as f64 / count as f64);
        RunningStats {
            count,
            mean,
            m2,
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    pub fn summary(&self) -> RunningStatsSummary {
        RunningStatsSummary {
            min: if self.count == 0 { 0.0 } else { self.min },
            max: if self.count == 0 { 0.0 } else { self.max },
            mean: self.mean,
            variance: self.variance(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RunningStatsSummary {
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub variance: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_batch_matches_naive_mean_variance() {
        let mut stats = RunningStats::default();
        for v in [1.0, 2.0, 3.0, 4.0] {
            stats.push(v);
        }
        assert!((stats.mean - 2.5).abs() < 1e-9);
        assert!((stats.variance() - 1.25).abs() < 1e-9);
        assert_eq!(stats.min, 1.0);
        assert_eq!(stats.max, 4.0);
    }

    #[test]
    fn merge_matches_single_pass_over_combined_data() {
        let mut a = RunningStats::default();
        for v in [1.0, 2.0, 3.0] {
            a.push(v);
        }
        let mut b = RunningStats::default();
        for v in [4.0, 5.0, 6.0] {
            b.push(v);
        }
        let merged = a.merge(&b);

        let mut whole = RunningStats::default();
        for v in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0] {
            whole.push(v);
        }
        assert!((merged.mean - whole.mean).abs() < 1e-9);
        assert!((merged.variance() - whole.variance()).abs() < 1e-9);
        assert_eq!(merged.min, whole.min);
        assert_eq!(merged.max, whole.max);
    }

    #[test]
    fn merge_with_empty_is_identity() {
        let mut a = RunningStats::default();
        a.push(10.0);
        let merged = a.merge(&RunningStats::default());
        assert_eq!(merged.count, 1);
        assert!((merged.mean - 10.0).abs() < 1e-9);
    }
}
