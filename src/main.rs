mod api;
mod bvid;
mod cli;
mod commands;
mod models;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let bili = api::Bili::new(cli.sessdata)?;

    match cli.command {
        Commands::Info { id } => commands::run_info(&bili, &id).await,
        Commands::Search { keyword, limit } => {
            commands::run_search(&bili, &keyword, limit).await
        }
        Commands::Links { id, quality, raw } => {
            commands::run_links(&bili, &id, quality, raw).await
        }
        Commands::Download {
            id,
            out_dir,
            quality,
            audio_only,
            no_merge,
        } => {
            commands::run_download(&bili, &id, &out_dir, quality, audio_only, no_merge).await
        }
        Commands::Subtitle {
            id,
            out,
            format,
            index,
            list,
        } => commands::run_subtitle(&bili, &id, out, &format, index, list).await,
    }
}
