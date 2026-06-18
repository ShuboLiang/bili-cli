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
    let json = cli.json;

    match cli.command {
        Commands::Info { id } => commands::run_info(&bili, &id, json).await,
        Commands::Search { keyword, limit } => {
            commands::run_search(&bili, &keyword, limit, json).await
        }
        Commands::Links { id, quality, raw } => {
            commands::run_links(&bili, &id, quality, raw, json).await
        }
        Commands::Download {
            id,
            out_dir,
            quality,
            audio_only,
            no_merge,
            page,
        } => {
            commands::run_download(&bili, &id, &out_dir, quality, audio_only, no_merge, page, json).await
        }
        Commands::Subtitle {
            id,
            out,
            format,
            index,
            list,
        } => commands::run_subtitle(&bili, &id, out, &format, index, list, json).await,
        Commands::Frames {
            id,
            out_dir,
            count,
            interval,
            at,
            source,
            format,
            page,
        } => {
            commands::run_frames(
                &bili, &id, &out_dir, count, interval, at, &source, &format, page, json,
            )
            .await
        }
        Commands::Transcript {
            id,
            page,
            start,
            end,
            max_chars,
            format,
            no_timestamps,
            out,
        } => {
            commands::run_transcript(
                &bili, &id, page, start, end, max_chars, &format, no_timestamps, out, json,
            )
            .await
        }
    }
}
