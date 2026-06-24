pub mod jsonl;
pub mod text;

pub use jsonl::JsonlSource;
pub use text::PlainTextSource;

use crate::config::{InputConfig, InputFormat};
use crate::record::RawRecord;
use globset::GlobBuilder;
use std::path::{Path, PathBuf};

pub trait RecordSource: Send {
    /// 1件読み込む。EOFはNoneで返す。
    fn next_record(&mut self) -> anyhow::Result<Option<RawRecord>>;
}

/// glob展開して該当するファイルパス一覧を返す。マッチしないパターンは警告のみ。
pub fn discover_input_files(paths: &[String]) -> anyhow::Result<Vec<PathBuf>> {
    let mut results = Vec::new();
    for pattern in paths {
        let direct = Path::new(pattern);
        if direct.is_file() {
            results.push(direct.to_path_buf());
            continue;
        }

        let base_dir = glob_base_dir(pattern);
        let base_dir = if base_dir.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            base_dir
        };
        if !base_dir.exists() {
            tracing::warn!("入力パスが見つかりません: {}", pattern);
            continue;
        }

        let matcher = GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()?
            .compile_matcher();

        let mut matched_any = false;
        for entry in walkdir::WalkDir::new(&base_dir)
            .into_iter()
            .filter_map(Result::ok)
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let rel = entry.path().to_string_lossy().replace('\\', "/");
            if matcher.is_match(&rel) {
                results.push(entry.path().to_path_buf());
                matched_any = true;
            }
        }
        if !matched_any {
            tracing::warn!("パターンに一致するファイルがありませんでした: {}", pattern);
        }
    }
    results.sort();
    results.dedup();
    Ok(results)
}

fn glob_base_dir(pattern: &str) -> PathBuf {
    let mut base = PathBuf::new();
    for comp in Path::new(pattern).components() {
        let s = comp.as_os_str().to_string_lossy();
        if s.contains('*') || s.contains('?') || s.contains('[') {
            break;
        }
        base.push(comp);
    }
    base
}

pub fn open_source(path: &Path, config: &InputConfig) -> anyhow::Result<Box<dyn RecordSource>> {
    match config.format {
        InputFormat::Text => Ok(Box::new(PlainTextSource::open(path, config.text_mode)?)),
        InputFormat::Jsonl => Ok(Box::new(JsonlSource::open(
            path,
            config.text_field.clone(),
            config.meta_fields.clone(),
        )?)),
        InputFormat::Warc | InputFormat::Html => {
            anyhow::bail!("input.format = {:?} はM1では未実装です", config.format)
        }
    }
}
