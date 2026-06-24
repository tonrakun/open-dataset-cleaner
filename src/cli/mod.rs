pub mod run;
pub mod validate;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "odc", version, about = "LLM学習データセット用クリーニングパイプライン")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Run(run::RunArgs),
    ValidateExtraction(validate::ValidateArgs),
}

pub fn dispatch() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Run(args) => run::execute(args),
        Command::ValidateExtraction(args) => validate::execute(args),
    }
}
