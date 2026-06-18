use anyhow::{anyhow, bail, Result};
use colored::Colorize;
use std::io::Write;
use std::path::PathBuf;

use crate::api::Bili;
use crate::commands::{fetch_player_subtitles, pick_best_subtitle, resolve};
use crate::models::SubtitleBody;

pub async fn run(
    bili: &Bili,
    raw: &str,
    page: usize,
    out: Option<PathBuf>,
    format: &str,
    index: usize,
    list_only: bool,
    json: bool,
) -> Result<()> {
    let (_id, info) = resolve(bili, raw).await?;
    let cid = crate::commands::cid_for_page(&info, page);

    let subs = fetch_player_subtitles(bili, &_id, cid, info.aid).await?;
    if subs.is_empty() {
        bail!("该视频没有可用字幕(可能需要登录 SESSDATA,或视频本身无字幕)");
    }

    if list_only {
        if json {
            crate::commands::print_json(&subs)?;
            return Ok(());
        }
        println!("{}", "可用字幕".bold().cyan());
        for (i, s) in subs.iter().enumerate() {
            let kind = if s.is_ai() { "AI" } else { "人工" };
            println!(
                "  [{i}] {lan:<10} {doc:<20} [{kind}]  ai_status={st}",
                i = i + 1,
                lan = s.lan,
                doc = s.lan_doc,
                kind = kind,
                st = s.ai_status
            );
        }
        return Ok(());
    }

    let chosen = if index > 0 {
        subs.get(index - 1)
            .ok_or_else(|| anyhow!("字幕索引 {index} 超出范围(共 {} 个)", subs.len()))?
    } else {
        pick_best_subtitle(&subs)
    };

    if !json {
        println!(
            "{} 选中: {} ({})",
            "字幕".cyan(),
            chosen.lan_doc.dimmed(),
            chosen.lan
        );
    }

    if chosen.subtitle_url.is_empty() {
        bail!("字幕 URL 为空(请重试或用 --list 查看字幕状态)");
    }

    let body = bili.fetch_subtitle(&chosen.subtitle_url).await?;
    if body.body.is_empty() {
        bail!("字幕内容为空");
    }

    let rendered = render(&body, format)?;
    match out {
        Some(p) => {
            let mut path = p.clone();
            // ensure extension matches format if user gave a bare name
            if path.extension().is_none() {
                path = path.with_extension(format);
            }
            let mut f = std::fs::File::create(&path)?;
            f.write_all(rendered.as_bytes())?;
            if !json {
                println!("{} {}", "已保存:".green(), path.display());
            } else {
                eprintln!("已保存: {}", path.display());
            }
        }
        None => {
            print!("{rendered}");
        }
    }
    Ok(())
}

fn render(body: &SubtitleBody, format: &str) -> Result<String> {
    match format.to_ascii_lowercase().as_str() {
        "json" => Ok(serde_json::to_string_pretty(body)?),
        "txt" => {
            let mut s = String::new();
            for l in &body.body {
                s.push_str(l.content.trim());
                s.push('\n');
            }
            Ok(s)
        }
        "srt" => Ok(to_srt(body)),
        "vtt" => Ok(to_vtt(body)),
        other => bail!("unsupported subtitle format: {other} (use srt/vtt/json/txt)"),
    }
}

fn fmt_ts_srt(ms: u128) -> String {
    let h = ms / 3_600_000;
    let m = (ms % 3_600_000) / 60_000;
    let s = (ms % 60_000) / 1_000;
    let ms = ms % 1_000;
    format!("{h:02}:{m:02}:{s:02},{ms:03}")
}

fn fmt_ts_vtt(ms: u128) -> String {
    let h = ms / 3_600_000;
    let m = (ms % 3_600_000) / 60_000;
    let s = (ms % 60_000) / 1_000;
    let ms = ms % 1_000;
    format!("{h:02}:{m:02}:{s:02}.{ms:03}")
}

fn to_srt(body: &SubtitleBody) -> String {
    let mut s = String::new();
    for (i, l) in body.body.iter().enumerate() {
        s.push_str(&format!("{}\n", i + 1));
        s.push_str(&format!(
            "{} --> {}\n",
            fmt_ts_srt(l.start_ms()),
            fmt_ts_srt(l.end_ms())
        ));
        s.push_str(l.content.trim());
        s.push_str("\n\n");
    }
    s
}

fn to_vtt(body: &SubtitleBody) -> String {
    let mut s = String::from("WEBVTT\n\n");
    for l in &body.body {
        s.push_str(&format!(
            "{} --> {}\n",
            fmt_ts_vtt(l.start_ms()),
            fmt_ts_vtt(l.end_ms())
        ));
        s.push_str(l.content.trim());
        s.push_str("\n\n");
    }
    s
}
