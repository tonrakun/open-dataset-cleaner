use arrow::array::{Array, StringArray};
use assert_cmd::Command;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use std::fs;

fn fixtures_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn run_odc(args: &[&str]) -> assert_cmd::assert::Assert {
    Command::cargo_bin("odc").unwrap().args(args).assert()
}

#[test]
fn run_jsonl_accepts_and_rejects_as_expected() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("out.jsonl");
    let stats = dir.path().join("out.stats.json");
    let config = fixtures_dir().join("configs/basic.toml");
    let input_glob = fixtures_dir().join("jsonl/sample.jsonl");

    run_odc(&[
        "run",
        "--config",
        config.to_str().unwrap(),
        "--input",
        input_glob.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--stats-output",
        stats.to_str().unwrap(),
    ])
    .success();

    let accepted_lines: Vec<String> = fs::read_to_string(&output)
        .unwrap()
        .lines()
        .map(|l| l.to_string())
        .collect();
    assert_eq!(accepted_lines.len(), 2, "ja/enの2件のみ採用されるはず");

    let rejected_path = std::path::PathBuf::from(format!("{}.rejected.jsonl", output.to_str().unwrap()));
    let rejected_lines: Vec<String> = fs::read_to_string(&rejected_path)
        .unwrap()
        .lines()
        .map(|l| l.to_string())
        .collect();
    assert_eq!(rejected_lines.len(), 2, "HTML残留行と重複行率超過の2件が除外されるはず");

    let stats_json: serde_json::Value = serde_json::from_str(&fs::read_to_string(&stats).unwrap()).unwrap();
    assert_eq!(stats_json["summary"]["total_input_records"], 4);
    assert_eq!(stats_json["summary"]["accepted_records"], 2);
    assert_eq!(stats_json["summary"]["rejected_records"], 2);
    assert_eq!(
        stats_json["rejection_reasons"]["residual_html_detected"],
        1
    );
    assert_eq!(
        stats_json["rejection_reasons"]["duplicate_line_ratio_exceeded"],
        1
    );
}

#[test]
fn run_and_or_not_combination_rule_rejects_matching_records() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("rules_out.jsonl");
    let stats = dir.path().join("rules_out.stats.json");
    let config = fixtures_dir().join("configs/rules.toml");
    let input_glob = fixtures_dir().join("jsonl/sample.jsonl");

    run_odc(&[
        "run",
        "--config",
        config.to_str().unwrap(),
        "--input",
        input_glob.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--stats-output",
        stats.to_str().unwrap(),
    ])
    .success();

    let accepted_lines: Vec<String> =
        fs::read_to_string(&output).unwrap().lines().map(|l| l.to_string()).collect();
    assert_eq!(accepted_lines.len(), 2, "ja/enの2件のみ採用されるはず");

    let stats_json: serde_json::Value = serde_json::from_str(&fs::read_to_string(&stats).unwrap()).unwrap();
    assert_eq!(stats_json["summary"]["total_input_records"], 4);
    assert_eq!(stats_json["summary"]["accepted_records"], 2);
    assert_eq!(stats_json["summary"]["rejected_records"], 2);
    assert_eq!(
        stats_json["rejection_reasons"]["custom_rule:unsupported_or_repetitive"],
        2,
        "残留HTML行と重複行率超過の両方がAND/OR/NOT組み合わせルールで除外されるはず"
    );
}

#[test]
fn run_dedup_rejects_exact_and_near_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("dedup_out.jsonl");
    let stats = dir.path().join("dedup_out.stats.json");
    let config = fixtures_dir().join("configs/dedup.toml");
    let input = fixtures_dir().join("jsonl/dedup.jsonl");

    run_odc(&[
        "run",
        "--config",
        config.to_str().unwrap(),
        "--input",
        input.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--stats-output",
        stats.to_str().unwrap(),
    ])
    .success();

    let accepted_lines: Vec<String> = fs::read_to_string(&output)
        .unwrap()
        .lines()
        .map(|l| l.to_string())
        .collect();
    assert_eq!(accepted_lines.len(), 2, "完全一致と近似重複の2件が除外され、残り2件のみ採用されるはず");

    let stats_json: serde_json::Value = serde_json::from_str(&fs::read_to_string(&stats).unwrap()).unwrap();
    assert_eq!(stats_json["summary"]["total_input_records"], 4);
    assert_eq!(stats_json["summary"]["accepted_records"], 2);
    assert_eq!(stats_json["rejection_reasons"]["duplicate_exact"], 1);
    assert_eq!(stats_json["rejection_reasons"]["duplicate_near_duplicate"], 1);
}

#[test]
fn run_is_idempotent_for_same_input_and_config() {
    let dir = tempfile::tempdir().unwrap();
    let config = fixtures_dir().join("configs/basic.toml");
    let input_glob = fixtures_dir().join("jsonl/sample.jsonl");

    let output1 = dir.path().join("run1.jsonl");
    let output2 = dir.path().join("run2.jsonl");

    for output in [&output1, &output2] {
        run_odc(&[
            "run",
            "--config",
            config.to_str().unwrap(),
            "--input",
            input_glob.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--stats-output",
            dir.path().join(format!("{}.stats.json", output.display())).to_str().unwrap(),
        ])
        .success();
    }

    let content1 = fs::read_to_string(&output1).unwrap();
    let content2 = fs::read_to_string(&output2).unwrap();
    assert_eq!(content1, content2, "同一入力・同一設定なら出力は一致するはず");
}

#[test]
fn run_with_no_matching_input_succeeds_with_empty_output() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("empty.jsonl");
    let config = fixtures_dir().join("configs/basic.toml");
    let no_match_glob = fixtures_dir().join("jsonl/does_not_exist_*.jsonl");

    run_odc(&[
        "run",
        "--config",
        config.to_str().unwrap(),
        "--input",
        no_match_glob.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--dry-run",
    ])
    .success();
}

#[test]
fn run_plain_text_newline_delimited() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("plain_out.jsonl");
    let config = fixtures_dir().join("configs/basic.toml");
    let input = fixtures_dir().join("plain/sample.txt");

    run_odc(&[
        "run",
        "--config",
        config.to_str().unwrap(),
        "--input",
        input.to_str().unwrap(),
        "--input-format",
        "text",
        "--output",
        output.to_str().unwrap(),
        "--stats-output",
        dir.path().join("plain.stats.json").to_str().unwrap(),
    ])
    .success();

    let lines: Vec<String> = fs::read_to_string(&output).unwrap().lines().map(|l| l.to_string()).collect();
    // 空行2行はスキップされ、残り3行が個別レコードとして採用される
    assert_eq!(lines.len(), 3);
}

#[test]
fn run_html_input_strips_boilerplate_and_converts_to_markdown() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("html_out.jsonl");
    let config = fixtures_dir().join("configs/html_markdown.toml");
    let input = fixtures_dir().join("html/sample.html");

    run_odc(&[
        "run",
        "--config",
        config.to_str().unwrap(),
        "--input",
        input.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--stats-output",
        dir.path().join("html.stats.json").to_str().unwrap(),
    ])
    .success();

    let lines: Vec<String> = fs::read_to_string(&output).unwrap().lines().map(|l| l.to_string()).collect();
    assert_eq!(lines.len(), 1, "本文記事1件が採用されるはず");
    let record: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
    let text = record["text"].as_str().unwrap();
    assert!(text.contains("# Welcome to Our Blog"), "見出しがMarkdown化されているはず: {text}");
    assert!(text.contains("- First important point"), "リストがMarkdown化されているはず: {text}");
    assert!(!text.contains("Home About Contact"), "navのボイラープレートは除去されるはず: {text}");
    assert!(!text.contains("Buy our product now"), "広告classのボイラープレートは除去されるはず: {text}");
    assert!(!text.contains("Copyright 2026"), "footerのボイラープレートは除去されるはず: {text}");
}

#[test]
fn run_jsonl_input_outputs_parquet() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("out.parquet");
    let config = fixtures_dir().join("configs/basic.toml");
    let input = fixtures_dir().join("jsonl/sample.jsonl");

    run_odc(&[
        "run",
        "--config",
        config.to_str().unwrap(),
        "--input",
        input.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--output-format",
        "parquet",
        "--stats-output",
        dir.path().join("parquet.stats.json").to_str().unwrap(),
    ])
    .success();

    let file = fs::File::open(&output).unwrap();
    let reader = ParquetRecordBatchReaderBuilder::try_new(file).unwrap().build().unwrap();
    let batches: Vec<_> = reader.map(|b| b.unwrap()).collect();
    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total_rows, 2, "ja/enの2件のみ採用されるはず");

    let ids: Vec<String> = batches
        .iter()
        .flat_map(|b| {
            let col = b.column_by_name("id").unwrap().as_any().downcast_ref::<StringArray>().unwrap();
            (0..col.len()).map(|i| col.value(i).to_string()).collect::<Vec<_>>()
        })
        .collect();
    assert!(ids.iter().any(|id| id.contains("sample.jsonl:1")));
    assert!(ids.iter().any(|id| id.contains("sample.jsonl:2")));

    let rejected_path = std::path::PathBuf::from(format!("{}.rejected.parquet", output.to_str().unwrap()));
    let rejected_file = fs::File::open(&rejected_path).unwrap();
    let rejected_reader = ParquetRecordBatchReaderBuilder::try_new(rejected_file).unwrap().build().unwrap();
    let rejected_rows: usize = rejected_reader.map(|b| b.unwrap().num_rows()).sum();
    assert_eq!(rejected_rows, 2, "HTML残留行と重複行率超過の2件が除外されるはず");
}
