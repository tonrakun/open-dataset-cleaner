use crate::extract::validate_extracted_text;
use crate::input::discover_input_files;
use clap::Args;

#[derive(Args, Debug)]
pub struct ValidateArgs {
    pub path: String,
    #[arg(long = "report", default_value = "text")]
    pub report: String,
}

pub fn execute(args: ValidateArgs) -> anyhow::Result<()> {
    crate::logging::init("info");
    let files = discover_input_files(std::slice::from_ref(&args.path))?;

    let mut total_files = 0u64;
    let mut files_with_html = 0u64;
    let mut files_with_url = 0u64;
    let mut html_samples = Vec::new();
    let mut url_samples = Vec::new();

    for file in &files {
        total_files += 1;
        let content = std::fs::read_to_string(file)
            .map_err(|e| anyhow::anyhow!("ファイルを読み込めません {}: {}", file.display(), e))?;
        let report = validate_extracted_text(&content);
        if report.has_residual_html_tags {
            files_with_html += 1;
            html_samples.extend(report.residual_html_tag_samples);
        }
        if report.has_residual_urls {
            files_with_url += 1;
            url_samples.extend(report.residual_url_samples);
        }
    }

    let html_preview: Vec<&String> = html_samples.iter().take(10).collect();
    let url_preview: Vec<&String> = url_samples.iter().take(10).collect();

    match args.report.as_str() {
        "json" => {
            let out = serde_json::json!({
                "total_files": total_files,
                "files_with_residual_html": files_with_html,
                "files_with_residual_url": files_with_url,
                "html_samples": html_preview,
                "url_samples": url_preview,
            });
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        _ => {
            println!(
                "total_files={} files_with_residual_html={} files_with_residual_url={}",
                total_files, files_with_html, files_with_url
            );
            if !html_preview.is_empty() {
                println!("html samples: {:?}", html_preview);
            }
            if !url_preview.is_empty() {
                println!("url samples: {:?}", url_preview);
            }
        }
    }
    Ok(())
}
