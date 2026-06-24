//! `perplexity` cargo feature配下のPerplexityスコアラー。
//!
//! KenLM(<https://kheafield.com/code/kenlm/>)のARPA/binary言語モデルを使い、
//! 文章の自然さを近似する perplexity を計算する想定のスコアラー。
//!
//! このファイルは**スキャフォルドのみ**であり、実際のKenLMバインディングへの
//! 接続は未実装。`PerplexityScorer::load` は常にエラーを返す。実装する場合は:
//!
//! 1. `Cargo.toml` の `perplexity` feature に、KenLMへバインドする依存クレート
//!    (もしくは独自の`kenlm-sys`風ラッパー)を `optional = true` で追加する。
//! 2. `PerplexityScorer` にロード済みモデルへのハンドルを保持させ、
//!    `load` でARPA/binaryファイルを読み込む。
//! 3. `score` で `model.perplexity(text)` 相当の計算を行い、
//!    `scores.perplexity` に格納する。
//!
//! ビルド環境にKenLM本体(C++ライブラリ、Boost等)のネイティブビルド設定が
//! 必要になる点に注意。

use super::{ScoreSet, Scorer};

/// KenLM言語モデルによるperplexityスコアラー(未実装スキャフォルド)。
#[derive(Debug)]
pub struct PerplexityScorer {
    model_path: String,
}

impl PerplexityScorer {
    /// KenLMモデルファイル(ARPA/binary)を読み込む。
    ///
    /// 実際のKenLMバインディングが接続されるまでは、設定の不整合を早期に
    /// 検知できるよう、呼び出し時点で明確なエラーを返す。
    pub fn load(model_path: &str) -> anyhow::Result<Self> {
        anyhow::bail!(
            "perplexity機能はスキャフォルドのみでKenLMバインディングが未接続のため使用できません \
             (kenlm_model_path={})。src/scoring/perplexity.rs にKenLMバインディングを実装してください。",
            model_path
        )
    }
}

impl Scorer for PerplexityScorer {
    fn name(&self) -> &'static str {
        "perplexity"
    }

    fn score(&self, _text: &str, _scores: &mut ScoreSet) -> anyhow::Result<()> {
        anyhow::bail!(
            "perplexity機能はスキャフォルドのみでKenLMバインディングが未接続のため使用できません \
             (model_path={})",
            self.model_path
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_returns_a_clear_not_implemented_error() {
        let err = PerplexityScorer::load("./model.bin").unwrap_err();
        assert!(err.to_string().contains("スキャフォルド"));
    }
}
