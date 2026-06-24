use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ThresholdConfig {
    #[serde(default)]
    pub reject_on_residual_html: bool,
    #[serde(default)]
    pub reject_on_residual_url: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FiltersConfig {
    #[serde(default)]
    pub thresholds: ThresholdConfig,
    #[serde(default)]
    pub rules: Vec<NamedRule>,
}

/// スコアフィールド上の比較演算子。
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    Lt,
    Lte,
    Gt,
    Gte,
    Eq,
    Ne,
    In,
    NotIn,
}

/// TOML上は数値・真偽値・文字列・文字列配列のいずれかとして表現される比較値。
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ConditionValue {
    Number(f64),
    Bool(bool),
    List(Vec<String>),
    Text(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct Condition {
    pub field: String,
    pub op: Op,
    pub value: ConditionValue,
}

/// AND/OR/NOTで条件を組み合わせるルール木。
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Rule {
    All { all: Vec<Rule> },
    Any { any: Vec<Rule> },
    Not { not: Box<Rule> },
    Cond(Condition),
}

/// 名前付きルール。`when` が真と評価されたレコードは `name` を除外理由として除外される。
#[derive(Debug, Clone, Deserialize)]
pub struct NamedRule {
    pub name: String,
    pub when: Rule,
}

/// 既知のスコアフィールド名一覧（設定検証時のタイプミス検出用）。
pub const KNOWN_RULE_FIELDS: &[&str] = &[
    "detected_language",
    "language_mixed_ratio",
    "duplicate_line_ratio",
    "symbol_digit_ratio",
    "avg_sentence_length",
    "sentence_length_variance",
    "has_residual_html",
    "has_residual_url",
    "hiragana_ratio",
    "katakana_ratio",
    "kanji_ratio",
    "alnum_ratio",
    "other_ratio",
];

impl Rule {
    fn validate(&self) -> anyhow::Result<()> {
        match self {
            Rule::All { all } | Rule::Any { any: all } => {
                for rule in all {
                    rule.validate()?;
                }
                Ok(())
            }
            Rule::Not { not } => not.validate(),
            Rule::Cond(cond) => {
                if !KNOWN_RULE_FIELDS.contains(&cond.field.as_str()) {
                    anyhow::bail!(
                        "filters.rules: 未知のフィールド名です: {} (使用可能: {})",
                        cond.field,
                        KNOWN_RULE_FIELDS.join(", ")
                    );
                }
                Ok(())
            }
        }
    }
}

impl FiltersConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        for rule in &self.rules {
            rule.when.validate().map_err(|e| anyhow::anyhow!("filters.rules[{}]: {}", rule.name, e))?;
        }
        Ok(())
    }
}
