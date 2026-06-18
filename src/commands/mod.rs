pub mod download;
pub mod info;
pub mod links;
pub mod search;
pub mod subtitle;

use anyhow::Result;
use colored::{ColoredString, Colorize};

pub fn human_count(n: u64) -> String {
    if n >= 100_000_000 {
        format!("{:.1}亿", n as f64 / 1e8)
    } else if n >= 10_000 {
        format!("{:.1}万", n as f64 / 1e4)
    } else {
        n.to_string()
    }
}

pub fn human_duration(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{}:{:02}:{:02}", h, m, s)
    } else {
        format!("{:02}:{:02}", m, s)
    }
}

/// Build a nice header like "  Title".
pub fn header(s: &str) -> ColoredString {
    s.bold().cyan()
}

pub use download::run as run_download;
pub use info::run as run_info;
pub use links::run as run_links;
pub use search::run as run_search;
pub use subtitle::run as run_subtitle;

/// Resolve a raw user id into (VideoId, VideoInfo) so commands can share logic.
pub async fn resolve(bili: &crate::api::Bili, raw: &str) -> Result<(crate::bvid::VideoId, crate::models::VideoInfo)> {
    let id = crate::bvid::parse_id(raw)?;
    let info = bili.video_info(&id).await?;
    Ok((id, info))
}
