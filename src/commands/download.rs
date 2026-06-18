use anyhow::{anyhow, bail, Context, Result};
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use tokio::process::Command;

use crate::api::Bili;
use crate::commands::links::pick_audio_candidates;
use crate::commands::resolve;
use crate::models::{Dash, DashStream};

pub async fn run(
    bili: &Bili,
    raw: &str,
    out_dir: &Path,
    quality: u32,
    audio_only: bool,
    no_merge: bool,
    page: usize,
    json: bool,
) -> Result<()> {
    let (id, info) = resolve(bili, raw).await?;
    let cid = crate::commands::cid_for_page(&info, page);
    let page_idx = if page == 0 { 0 } else { page - 1 };
    let part = info.pages.get(page_idx).map(|p| p.part.as_str()).unwrap_or("");

    let bundle = bili.play_url(&id, cid, quality).await?;
    let dash = bundle
        .dash
        .as_ref()
        .ok_or_else(|| anyhow!("this video has no DASH stream (single-part mp4 only). Try a different video."))?;

    let safe_title = sanitize(&info.title);
    let file_suffix = if !part.is_empty() {
        format!("P{:02}_{}", page.max(1), sanitize(part))
    } else {
        String::new()
    };
    let base = out_dir.join(format!("{}_{}{}", id.label(), safe_title, if file_suffix.is_empty() { String::new() } else { format!("_{}", file_suffix) }));
    tokio::fs::create_dir_all(out_dir).await.ok();

    let style = if json {
        None
    } else {
        Some(
            ProgressStyle::with_template(
                "{spinner:.green} {prefix:<8} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec})",
            )
            .unwrap()
            .progress_chars("=>-"),
        )
    };
    let mk_bar = |prefix: &str| -> Option<ProgressBar> {
        style.as_ref().map(|s| {
            let pb = ProgressBar::new(0);
            pb.set_style(s.clone());
            pb.set_prefix(prefix.to_string());
            pb
        })
    };
    let status = |msg: String| {
        if json {
            eprintln!("{msg}");
        } else {
            println!("{msg}");
        }
    };

    let v_path = base.with_extension("video.mp4");
    let a_path = base.with_extension("audio.m4a");
    let out_path = base.with_extension("mp4");

    let mut merged: Option<String> = None;
    let mut video_file: Option<String> = None;
    let mut audio_file: Option<String> = None;

    if audio_only {
        let candidates = pick_audio_candidates(dash);
        if candidates.is_empty() {
            bail!("no audio stream");
        }
        let audio = download_audio_with_fallback(bili, &candidates, &a_path, &style, json)
            .await
            .context("download audio")?;
        audio_file = Some(a_path.to_string_lossy().to_string());
        if !json {
            println!("\n{} {}", "已保存:".green(), a_path.display());
        }
        return finish(json, &info, cid, audio.id, audio_only, merged, video_file, audio_file, false);
    }

    let video = pick_video(dash, quality)?;
    let audio_candidates = pick_audio_candidates(dash);

    if !json {
        println!(
            "{} 视频 qn={} {}x{} {}",
            "选择".cyan(),
            video.id,
            video.width.unwrap_or(0),
            video.height.unwrap_or(0),
            crate::commands::links::codec_name(video.codecid)
        );
    }

    if let Some(pb) = mk_bar("video") {
        bili.download_to_file(&video.base_url, &v_path, Some(&pb))
            .await
            .context("download video")?;
        pb.finish_with_message("video done");
    } else {
        bili.download_to_file(&video.base_url, &v_path, None)
            .await
            .context("download video")?;
    }

    let audio: Option<&DashStream> = if audio_candidates.is_empty() {
        None
    } else {
        match download_audio_with_fallback(bili, &audio_candidates, &a_path, &style, json).await {
            Ok(a) => Some(a),
            Err(e) => {
                if !json {
                    eprintln!("{} 所有音频候选失败: {}", "警告".yellow(), e);
                }
                None
            }
        }
    };

    let ff_avail = ffmpeg_available();

    if no_merge || audio.is_none() {
        video_file = Some(v_path.to_string_lossy().to_string());
        if audio.is_some() {
            audio_file = Some(a_path.to_string_lossy().to_string());
        }
        if !json {
            println!("\n{} {}", "已保存(视频):".green(), v_path.display());
            if audio.is_some() {
                println!("{} {}", "已保存(音频):".green(), a_path.display());
            }
        }
        return finish(json, &info, cid, video.id, audio_only, merged, video_file, audio_file, false);
    }

    // merge with ffmpeg
    if !ff_avail {
        status(format!(
            "{} 未找到 ffmpeg,跳过合并。视频/音频分别保存在:\n  {}\n  {}\n请安装 ffmpeg 后重新运行,或手动合并。",
            "警告:".yellow(),
            v_path.display(),
            a_path.display()
        ));
        video_file = Some(v_path.to_string_lossy().to_string());
        audio_file = Some(a_path.to_string_lossy().to_string());
        return finish(json, &info, cid, video.id, audio_only, merged, video_file, audio_file, false);
    }

    if !json {
        println!("\n{} 用 ffmpeg 合并音视频...", "合并".cyan());
    }
    merge_with_ffmpeg(&v_path, &a_path, &out_path).await?;
    merged = Some(out_path.to_string_lossy().to_string());
    if !json {
        println!("{} {}", "完成:".green(), out_path.display());
    }

    // cleanup intermediates
    let _ = tokio::fs::remove_file(&v_path).await;
    let _ = tokio::fs::remove_file(&a_path).await;

    finish(json, &info, cid, video.id, audio_only, merged, video_file, audio_file, true)
}

fn finish(
    json: bool,
    info: &crate::models::VideoInfo,
    cid: u64,
    qn: u32,
    audio_only: bool,
    merged: Option<String>,
    video_file: Option<String>,
    audio_file: Option<String>,
    ffmpeg_merged: bool,
) -> Result<()> {
    if json {
        let payload = serde_json::json!({
            "video": {
                "bvid": info.bvid,
                "aid": info.aid,
                "title": info.title,
            },
            "cid": cid,
            "quality": qn,
            "audio_only": audio_only,
            "files": {
                "merged": merged,
                "video": video_file,
                "audio": audio_file,
            },
            "ffmpeg_merged": ffmpeg_merged,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
    }
    Ok(())
}

fn pick_video(dash: &Dash, want_qn: u32) -> Result<&crate::models::DashStream> {
    crate::commands::links::pick_video(dash, want_qn)
}

fn ffmpeg_available() -> bool {
    std::process::Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

async fn merge_with_ffmpeg(v: &Path, a: &Path, out: &Path) -> Result<()> {
    let out_str = out
        .to_str()
        .ok_or_else(|| anyhow!("output path is not utf-8"))?;
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-i",
            v.to_str().unwrap(),
            "-i",
            a.to_str().unwrap(),
            "-c:v",
            "copy",
            "-c:a",
            "copy",
            "-movflags",
            "+faststart",
            out_str,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .context("run ffmpeg")?;
    if !status.success() {
        bail!("ffmpeg exited with status {status}; see output above");
    }
    Ok(())
}

/// Try audio candidates in priority order (Dolby → Hi-Res → regular), falling
/// back to the next stream when the CDN rejects one (e.g. logged-out users
/// hitting Dolby URLs get HTTP 404). Returns the stream that downloaded OK.
async fn download_audio_with_fallback<'a>(
    bili: &Bili,
    candidates: &[&'a DashStream],
    a_path: &Path,
    style: &Option<ProgressStyle>,
    json: bool,
) -> Result<&'a DashStream> {
    let mk_bar = |prefix: &str| -> Option<ProgressBar> {
        style.as_ref().map(|s| {
            let pb = ProgressBar::new(0);
            pb.set_style(s.clone());
            pb.set_prefix(prefix.to_string());
            pb
        })
    };

    let mut last_err: Option<anyhow::Error> = None;
    for (i, audio) in candidates.iter().copied().enumerate() {
        if !json {
            if i == 0 {
                println!(
                    "{} 音频流: {}kbps ({})",
                    "选择".cyan(),
                    audio.bandwidth / 1000,
                    audio.codecs
                );
            } else {
                eprintln!(
                    "{} 降级到音频流: {}kbps ({})",
                    "回退".yellow(),
                    audio.bandwidth / 1000,
                    audio.codecs
                );
            }
        }
        let pb = mk_bar("audio");
        match bili
            .download_to_file(&audio.base_url, a_path, pb.as_ref())
            .await
        {
            Ok(()) => {
                if let Some(pb) = pb {
                    pb.finish_with_message("audio done");
                }
                return Ok(audio);
            }
            Err(e) => {
                if let Some(pb) = pb {
                    pb.finish_with_message("audio failed");
                }
                if !json {
                    eprintln!("{} 候选失败: {},尝试下一音频流", "降级".yellow(), e);
                }
                let _ = tokio::fs::remove_file(a_path).await;
                last_err = Some(e);
            }
        }
    }
    Err(last_err
        .map(|e| e.context(format!("all {} audio candidates failed", candidates.len())))
        .unwrap_or_else(|| anyhow!("no audio candidates available")))
}

fn sanitize(s: &str) -> String {
    let bad = ['/', '\\', ':', '*', '?', '"', '<', '>', '|', '\0'];
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if bad.contains(&c) {
            out.push('_');
        } else if c.is_control() {
            continue;
        } else {
            out.push(c);
        }
    }
    let trimmed = out.trim();
    if trimmed.is_empty() {
        "video".to_string()
    } else {
        let cut: String = trimmed.chars().take(80).collect();
        cut
    }
}
