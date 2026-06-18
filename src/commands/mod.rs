pub mod download;
pub mod info;
pub mod links;
pub mod search;
pub mod subtitle;
pub mod transcript;

use anyhow::Result;
use colored::{ColoredString, Colorize};
use serde::Serialize;

use crate::models::SubtitleMeta;

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

/// Pretty-print a JSON value to stdout (for `--json` mode).
pub fn print_json<T: Serialize>(v: &T) -> Result<()> {
    let s = serde_json::to_string_pretty(v)?;
    println!("{s}");
    Ok(())
}

/// Pick the "smart" best subtitle: prefer manual zh-Hans, then zh-Hant, zh,
/// then AI-generated zh, then english, then the first one.
pub fn pick_best_subtitle(subs: &[SubtitleMeta]) -> &SubtitleMeta {
    let order: &[(&str, bool)] = &[
        ("zh-Hans", false),
        ("zh-CN", false),
        ("zh-Hant", false),
        ("zh-TW", false),
        ("zh", false),
        ("zh-Hans", true),
        ("ai-zh", true),
        ("en", false),
        ("en", true),
    ];
    for (lang, allow_ai) in order {
        if let Some(m) = subs
            .iter()
            .find(|s| s.lan.eq_ignore_ascii_case(lang) && (s.ai_type != 0) == *allow_ai)
        {
            return m;
        }
    }
    subs.iter()
        .find(|s| s.ai_type == 0)
        .unwrap_or(&subs[0])
}

pub use download::run as run_download;
pub use info::run as run_info;
pub use links::run as run_links;
pub use search::run as run_search;
pub use subtitle::run as run_subtitle;
pub use transcript::run as run_transcript;

/// Resolve a raw user id into (VideoId, VideoInfo) so commands can share logic.
pub async fn resolve(
    bili: &crate::api::Bili,
    raw: &str,
) -> Result<(crate::bvid::VideoId, crate::models::VideoInfo)> {
    let id = crate::bvid::parse_id(raw)?;
    let info = bili.video_info(&id).await?;
    Ok((id, info))
}

/// Select the cid for a given 1-based page index from VideoInfo.
/// Falls back to the top-level `cid` when there are no sub-pages or the
/// index is out of range.
pub fn cid_for_page(info: &crate::models::VideoInfo, page: usize) -> u64 {
    if info.pages.is_empty() {
        return info.cid;
    }
    let idx = if page == 0 { 0 } else { page - 1 };
    info.pages
        .get(idx)
        .map(|p| p.cid)
        .unwrap_or(info.pages[0].cid)
}
