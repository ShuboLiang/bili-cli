use anyhow::{anyhow, bail, Context, Result};
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use tokio::process::Command;

use crate::api::Bili;
use crate::commands::resolve;
use crate::models::Dash;

pub async fn run(
    bili: &Bili,
    raw: &str,
    out_dir: &Path,
    quality: u32,
    audio_only: bool,
    no_merge: bool,
) -> Result<()> {
    let (id, info) = resolve(bili, raw).await?;
    let cid = info.pages.first().map(|p| p.cid).unwrap_or(info.cid);

    let bundle = bili.play_url(&id, cid, quality).await?;
    let dash = bundle
        .dash
        .as_ref()
        .ok_or_else(|| anyhow!("this video has no DASH stream (single-part mp4 only). Try a different video."))?;

    let safe_title = sanitize(&info.title);
    let base = out_dir.join(format!("{}_{}", id.label(), safe_title));
    tokio::fs::create_dir_all(out_dir).await.ok();

    let style = ProgressStyle::with_template(
        "{spinner:.green} {prefix:<8} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec})",
    )
    .unwrap()
    .progress_chars("=>-");

    let v_path = base.with_extension("video.mp4");
    let a_path = base.with_extension("audio.m4a");
    let out_path = base.with_extension("mp4");

    if audio_only {
        let audio = pick_audio(dash)?;
        println!("{} 音频流: {}kbps ({})", "选择".cyan(), audio.bandwidth / 1000, audio.codecs);
        let pb = ProgressBar::new(0);
        pb.set_style(style.clone());
        pb.set_prefix("audio");
        bili.download_to_file(&audio.base_url, &a_path, Some(&pb))
            .await
            .context("download audio")?;
        pb.finish_with_message("audio done");
        println!("\n{} {}", "已保存:".green(), a_path.display());
        return Ok(());
    }

    let video = pick_video(dash, quality)?;
    let audio = pick_audio(dash).ok();

    println!(
        "{} 视频 qn={} {}x{} {}",
        "选择".cyan(),
        video.id,
        video.width.unwrap_or(0),
        video.height.unwrap_or(0),
        crate::commands::links::codec_name(video.codecid)
    );

    let vpb = ProgressBar::new(0);
    vpb.set_style(style.clone());
    vpb.set_prefix("video");
    bili.download_to_file(&video.base_url, &v_path, Some(&vpb))
        .await
        .context("download video")?;
    vpb.finish_with_message("video done");

    if let Some(a) = audio {
        let apb = ProgressBar::new(0);
        apb.set_style(style.clone());
        apb.set_prefix("audio");
        bili.download_to_file(&a.base_url, &a_path, Some(&apb))
            .await
            .context("download audio")?;
        apb.finish_with_message("audio done");
    }

    if no_merge || audio.is_none() {
        println!("\n{} {}", "已保存(视频):".green(), v_path.display());
        if audio.is_some() {
            println!("{} {}", "已保存(音频):".green(), a_path.display());
        }
        return Ok(());
    }

    // merge with ffmpeg
    if !ffmpeg_available() {
        eprintln!(
            "{} 未找到 ffmpeg,跳过合并。视频/音频分别保存在:\n  {}\n  {}\n请安装 ffmpeg 后重新运行,或手动合并。",
            "警告:".yellow(),
            v_path.display(),
            a_path.display()
        );
        return Ok(());
    }

    println!("\n{} 用 ffmpeg 合并音视频...", "合并".cyan());
    merge_with_ffmpeg(&v_path, &a_path, &out_path).await?;
    println!("{} {}", "完成:".green(), out_path.display());

    // cleanup intermediates
    let _ = tokio::fs::remove_file(&v_path).await;
    let _ = tokio::fs::remove_file(&a_path).await;
    Ok(())
}

fn pick_video(dash: &Dash, want_qn: u32) -> Result<&crate::models::DashStream> {
    crate::commands::links::pick_video(dash, want_qn)
}

fn pick_audio(dash: &Dash) -> Result<&crate::models::DashStream> {
    crate::commands::links::pick_best_audio(dash)
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
