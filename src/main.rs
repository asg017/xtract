mod cli;
mod commands;
mod js_runner;
mod markdown;
mod pages;
mod progress;
mod sqlite;

use clap::Parser;
use commands::extract;

fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();

    match cli.command {
        cli::Command::Schema { file } => commands::schema::run(&file),
        cli::Command::Check { file } => commands::check::run(&file),
        cli::Command::Extract {
            schema,
            inputs,
            prompt,
            model,
            provider,
            page,
            pages,
            screenshot,
            name,
            output,
            concurrency,
            force,
        } => extract::run_command(extract::CommandArgs {
            schema,
            inputs,
            prompt,
            model,
            provider,
            page,
            pages,
            screenshot,
            name,
            output,
            concurrency,
            force,
        }),
    }
}
