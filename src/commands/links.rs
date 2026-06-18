use anyhow::{bail, Result};
use colored::Colorize;
use tabled::{Table, Tabled};

use crate::api::Bili;
use crate::commands::resolve;
use crate::models::{Dash, DashStream, PlayUrlBundle};

#[derive(Tabled)]
struct VRow<'a> {
    #[tabled(rename = "qn")]
    id: u32,
    #[tabled(rename = "清晰度")]
    desc: String,
    #[tabled(rename = "编码")]
    codec: &'a str,
    #[tabled(rename = "分辨率")]
    res: String,
    #[tabled(rename = "码率(kbps)")]
    bw: String,
}

pub async fn run(bili: &Bili, raw: &str, quality: u32, raw_only: bool, json: bool) -> Result<()> {
    let (id, info) = resolve(bili, raw).await?;
    let cid = info.pages.first().map(|p| p.cid).unwrap_or(info.cid);

    let bundle = bili.play_url(&id, cid, quality).await?;
    if json {
        return crate::commands::print_json(&bundle);
    }
    print_bundle(&bundle, raw_only)?;
    Ok(())
}

pub fn print_bundle(bundle: &PlayUrlBundle, raw_only: bool) -> Result<()> {
    match &bundle.dash {
        Some(dash) => print_dash(dash, raw_only),
        None => print_durl(bundle, raw_only),
    }
}

fn print_durl(bundle: &PlayUrlBundle, raw_only: bool) -> Result<()> {
    let durls = bundle
        .durl
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no durl and no dash in response"))?;
    if raw_only {
        for d in durls {
            println!("{}", d.url);
        }
    } else {
        println!("{}", "单段流 (mp4/flv)".bold().cyan());
        for d in durls {
            println!(
                "  order={}  size={} MB  url={}",
                d.order,
                d.size / 1_000_000,
                d.url
            );
        }
    }
    Ok(())
}

fn print_dash(dash: &Dash, raw_only: bool) -> Result<()> {
    if dash.video.is_empty() {
        bail!("no video streams in dash response");
    }

    // map qn -> description
    let desc_map: std::collections::HashMap<u32, &str> = [
        (127u32, "8K 超高清"),
        (126, "杜比视界"),
        (125, "HDR 真彩"),
        (120, "4K 超清"),
        (116, "1080P 60帧"),
        (112, "1080P 高码率"),
        (100, "1080P 高清"),
        (74, "720P 60帧"),
        (64, "720P 高清"),
        (32, "480P 流畅"),
        (16, "360P 流畅"),
        (6, "240P"),
        (0, "未知"),
    ]
    .into_iter()
    .collect();

    if raw_only {
        for v in &dash.video {
            println!("# video qn={} {} {}", v.id, codec_name(v.codecid), v.base_url);
        }
        for a in &dash.audio {
            println!("# audio {} {}", a.id, a.base_url);
        }
        return Ok(());
    }

    // For a compact table, show one row per (qn, codec) combination, highest bandwidth first.
    let mut streams: Vec<&DashStream> = dash.video.iter().collect();
    streams.sort_by(|a, b| b.id.cmp(&a.id).then(b.bandwidth.cmp(&a.bandwidth)));

    let mut seen = std::collections::HashSet::<(u32, u32)>::new();
    let rows: Vec<VRow> = streams
        .into_iter()
        .filter(|v| seen.insert((v.id, v.codecid)))
        .map(|v| VRow {
            id: v.id,
            desc: desc_map.get(&v.id).copied().unwrap_or("其它").to_string(),
            codec: codec_name(v.codecid),
            res: match (v.width, v.height) {
                (Some(w), Some(h)) => format!("{}x{}", w, h),
                _ => "-".into(),
            },
            bw: (v.bandwidth / 1000).to_string(),
        })
        .collect();

    let table = Table::new(rows);
    println!("{}\n{}", "视频流".bold().cyan(), table);

    if !dash.audio.is_empty() {
        println!("\n{}", "音频流".bold().cyan());
        for a in &dash.audio {
            println!(
                "  id={}  {}  {}kbps",
                a.id,
                a.codecs,
                a.bandwidth / 1000
            );
        }
    }

    println!("\n{} 使用 `bili-cli download <id>` 下载,或加 --audio-only 仅取音频。", "提示:".yellow());
    Ok(())
}

pub fn codec_name(codecid: u32) -> &'static str {
    match codecid {
        7 => "AV1",
        12 => "HEVC",
        13 => "AV1",
        _ => "AVC/H.264",
    }
}

/// Pick the best video stream for a target quality (or the highest available).
pub fn pick_video(dash: &Dash, want_qn: u32) -> Result<&DashStream> {
    let mut streams: Vec<&DashStream> = dash.video.iter().collect();
    if streams.is_empty() {
        bail!("no video streams");
    }
    streams.sort_by(|a, b| b.bandwidth.cmp(&a.bandwidth));
    if want_qn == 0 {
        return Ok(streams[0]);
    }
    streams
        .into_iter()
        .find(|s| s.id == want_qn)
        .or_else(|| {
            // fall back to nearest lower quality
            let mut all: Vec<&DashStream> = dash.video.iter().collect();
            all.sort_by_key(|s| s.id);
            all.into_iter().rev().find(|s| s.id <= want_qn)
        })
        .ok_or_else(|| anyhow::anyhow!("no matching video stream for qn={want_qn}"))
}

/// Return all audio candidates ordered by preference: Dolby (highest bw first),
/// then Hi-Res FLAC, then regular audio (highest bw first). Used for fallback
/// downloads when the top-priority stream 404s (common for logged-out users
/// hitting Dolby/Hi-Res CDN URLs).
pub fn pick_audio_candidates(dash: &Dash) -> Vec<&DashStream> {
    let mut out: Vec<&DashStream> = Vec::new();
    if let Some(dolby) = &dash.dolby {
        if let Some(aud) = dolby.audio.as_ref() {
            let mut sorted: Vec<&DashStream> = aud.iter().collect();
            sorted.sort_by(|a, b| b.bandwidth.cmp(&a.bandwidth));
            out.extend(sorted);
        }
    }
    if let Some(flac) = &dash.flac {
        if let Some(aud) = &flac.audio {
            out.push(aud);
        }
    }
    let mut regular: Vec<&DashStream> = dash.audio.iter().collect();
    regular.sort_by(|a, b| b.bandwidth.cmp(&a.bandwidth));
    out.extend(regular);
    out
}
