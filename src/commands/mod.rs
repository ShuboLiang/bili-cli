pub mod download;
pub mod frames;
pub mod info;
pub mod links;
pub mod search;
pub mod subtitle;
pub mod transcript;

use anyhow::Result;
use colored::{ColoredString, Colorize};
use serde::Serialize;

use crate::api::Bili;
use crate::bvid::VideoId;
use crate::models::{SubtitleBody, SubtitleMeta};

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
        ("zh", true),
        ("en", false),
        ("en", true),
    ];
    for (lang, allow_ai) in order {
        if let Some(m) = subs
            .iter()
            .find(|s| s.lan.eq_ignore_ascii_case(lang) && s.is_ai() == *allow_ai)
        {
            return m;
        }
    }
    subs.iter()
        .find(|s| !s.is_ai())
        .unwrap_or(&subs[0])
}

pub use download::run as run_download;
pub use frames::run as run_frames;
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

/// Fetch player subtitles for a page, retrying up to 5 times. Bilibili's
/// player/v2 endpoint has a server-side bug where AI subtitle URLs
/// intermittently point to *other videos'* subtitles. We validate each
/// URL's path contains the expected `aid`+`cid` and skip mismatches.
pub async fn fetch_player_subtitles(
    bili: &Bili,
    id: &VideoId,
    cid: u64,
    aid: u64,
) -> Result<Vec<SubtitleMeta>> {
    let aid_str = aid.to_string();
    let cid_str = cid.to_string();
    let mut best: Vec<SubtitleMeta> = Vec::new();
    let mut best_score = -1i32;

    for _ in 0..5 {
        match bili.player_view(id, cid).await {
            Ok(view) => {
                if let Some(sub) = view.subtitle.as_ref() {
                    if sub.subtitles.is_empty() {
                        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                        continue;
                    }
                    // Score this batch: how many URLs contain both aid and cid?
                    let score = sub.subtitles.iter().filter(|s| {
                        s.subtitle_url.contains(&aid_str) && s.subtitle_url.contains(&cid_str)
                    }).count() as i32;
                    if score > best_score {
                        best_score = score;
                        best = sub.subtitles.clone();
                    }
                    if score > 0 {
                        return Ok(best);
                    }
                }
            }
            Err(_) => {}
        }
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    }
    Ok(best)
}

/// Pick the best subtitle that has a non-empty URL whose path contains the
/// expected `aid`+`cid` (guarding against Bilibili's server-side subtitle
/// URL mix-up bug). Retries the API a few times. Returns
/// (chosen_subtitle, subtitle_body) or None when truly no subtitle.
pub async fn fetch_best_subtitle(
    bili: &Bili,
    id: &VideoId,
    cid: u64,
    aid: u64,
) -> Result<Option<(SubtitleMeta, SubtitleBody)>> {
    let aid_str = aid.to_string();
    let cid_str = cid.to_string();

    for attempt in 0..5 {
        let subs = fetch_player_subtitles(bili, id, cid, aid).await?;
        if subs.is_empty() {
            if attempt < 4 {
                tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            }
            continue;
        }

        // First pass: pick best subtitle whose URL is valid (contains aid+cid)
        for s in &subs {
            if s.subtitle_url.is_empty() {
                continue;
            }
            if !s.subtitle_url.contains(&aid_str) || !s.subtitle_url.contains(&cid_str) {
                continue;
            }
            match bili.fetch_subtitle(&s.subtitle_url).await {
                Ok(body) if !body.body.is_empty() => {
                    return Ok(Some((s.clone(), body)));
                }
                _ => {}
            }
        }

        // Second pass: try any non-empty URL (last resort, might be wrong video)
        if attempt == 4 {
            let chosen = pick_best_subtitle(&subs);
            if !chosen.subtitle_url.is_empty() {
                if let Ok(body) = bili.fetch_subtitle(&chosen.subtitle_url).await {
                    if !body.body.is_empty() {
                        return Ok(Some((chosen.clone(), body)));
                    }
                }
            }
        }

        if attempt < 4 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }
    Ok(None)
}
