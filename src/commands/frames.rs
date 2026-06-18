use anyhow::{anyhow, bail, Context, Result};
use colored::Colorize;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tokio::process::Command;

use crate::api::Bili;
use crate::commands::{cid_for_page, resolve};

pub async fn run(
    bili: &Bili,
    raw: &str,
    out_dir: &Path,
    count: Option<usize>,
    interval: Option<f64>,
    at: Option<String>,
    source: &str,
    format: &str,
    quality: u32,
    page: usize,
    json: bool,
) -> Result<()> {
    let (id, info) = resolve(bili, raw).await?;
    let cid = cid_for_page(&info, page);
    // use the specific page's duration, not the whole video's (multi-part fix)
    let page_idx = if page == 0 { 0 } else { page - 1 };
    let page_duration = info
        .pages
        .get(page_idx)
        .map(|p| p.duration)
        .unwrap_or(info.duration);
    let duration = page_duration.max(1) as f64;

    // compute target timestamps
    let timestamps = compute_timestamps(count, interval, at, duration)?;
    if timestamps.is_empty() {
        bail!("no timestamps to extract (check --count/--interval/--at)");
    }

    if !json {
        let part = info.pages.get(page_idx).map(|p| p.part.as_str()).unwrap_or("");
        eprintln!(
            "{} {} | {} | P{} {} | 时长 {}s | 取 {} 帧",
            "视频".cyan(),
            info.title.dimmed(),
            id.label(),
            page.max(1),
            part,
            page_duration,
            timestamps.len()
        );
    }

    tokio::fs::create_dir_all(out_dir).await.ok();

    let use_storyboard = source == "auto" || source == "storyboard";
    let use_ffmpeg = source == "auto" || source == "ffmpeg";

    let result: FrameResult = if use_storyboard {
        match try_storyboard(bili, &id, cid, &timestamps, &info, duration, out_dir, format, json).await {
            Ok(r) => r,
            Err(e) => {
                if !json {
                    eprintln!("{} 雪碧图不可用 ({}),尝试 ffmpeg...", "降级".yellow(), e);
                }
                if !use_ffmpeg {
                    return Err(anyhow!("storyboard unavailable and --source=storyboard: {e}"));
                }
                try_ffmpeg(bili, &id, cid, &timestamps, &info, out_dir, format, quality, json).await?
            }
        }
    } else {
        try_ffmpeg(bili, &id, cid, &timestamps, &info, out_dir, format, quality, json).await?
    };

    if json {
        let payload = FrameJson {
            video: VideoRef {
                bvid: info.bvid.clone(),
                aid: info.aid,
                title: info.title.clone(),
                duration: duration as u64,
            },
            cid,
            source: result.source.clone(),
            count: result.frames.len(),
            frames: result
                .frames
                .iter()
                .map(|f| FrameRef {
                    timestamp: f.timestamp,
                    file: f.file.to_string_lossy().to_string(),
                })
                .collect(),
        };
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        eprintln!("{} 来源: {}", "完成".green(), result.source);
        for f in &result.frames {
            println!(
                "  [{:>7}] {}",
                format!("{:.1}s", f.timestamp),
                f.file.display()
            );
        }
    }
    Ok(())
}

// ---------- types ----------

struct FrameResult {
    source: String,
    frames: Vec<ExtractedFrame>,
}

struct ExtractedFrame {
    timestamp: f64,
    file: PathBuf,
}

#[derive(Serialize)]
struct FrameJson {
    video: VideoRef,
    cid: u64,
    source: String,
    count: usize,
    frames: Vec<FrameRef>,
}

#[derive(Serialize)]
struct VideoRef {
    bvid: String,
    aid: u64,
    title: String,
    duration: u64,
}

#[derive(Serialize)]
struct FrameRef {
    timestamp: f64,
    file: String,
}

// ---------- timestamp computation ----------

fn compute_timestamps(
    count: Option<usize>,
    interval: Option<f64>,
    at: Option<String>,
    duration: f64,
) -> Result<Vec<f64>> {
    let n_specified = [count.is_some(), interval.is_some(), at.is_some()]
        .iter()
        .filter(|&&b| b)
        .count();
    if n_specified > 1 {
        bail!("--count, --interval, --at are mutually exclusive; pick one");
    }

    if let Some(at_str) = at {
        let ts: Vec<f64> = at_str
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if ts.is_empty() {
            bail!("--at: no valid timestamps parsed from '{at_str}'");
        }
        return Ok(ts);
    }

    if let Some(sec) = interval {
        if sec <= 0.0 {
            bail!("--interval must be > 0");
        }
        let mut ts = Vec::new();
        let mut t = 0.0;
        while t < duration {
            ts.push(t);
            t += sec;
        }
        return Ok(ts);
    }

    let n = count.unwrap_or(8).max(1);
    let mut ts = Vec::with_capacity(n);
    if n == 1 {
        ts.push(0.0);
    } else {
        for i in 0..n {
            let t = duration * i as f64 / (n - 1) as f64;
            // clamp to avoid exact end-of-video (ffmpeg can't seek past last frame)
            ts.push(t.min(duration - 0.5));
        }
    }
    Ok(ts)
}

// ---------- storyboard path ----------

async fn try_storyboard(
    bili: &Bili,
    id: &crate::bvid::VideoId,
    cid: u64,
    timestamps: &[f64],
    info: &crate::models::VideoInfo,
    duration: f64,
    out_dir: &Path,
    format: &str,
    json: bool,
) -> Result<FrameResult> {
    let shot = bili.videoshot(id, cid).await?;
    if shot.image.is_empty() || shot.img_x_len == 0 || shot.img_y_len == 0 {
        bail!("no sprite sheet available");
    }

    let total_frames = shot.total_frames();
    if total_frames == 0 {
        bail!("sprite sheet has 0 frames");
    }
    let fps = total_frames as f64 / duration; // frames per second

    // download all sprite sheets to temp files
    let tmp = std::env::temp_dir().join(format!("bili-cli-sprite-{}", cid));
    tokio::fs::create_dir_all(&tmp).await.ok();
    let mut sprite_paths: Vec<PathBuf> = Vec::new();
    for (i, url) in shot.image.iter().enumerate() {
        let path = tmp.join(format!("sprite_{i}.jpg"));
        let pb = if json { None } else { None };
        bili.download_to_file(url, &path, pb.as_ref())
            .await
            .with_context(|| format!("download sprite sheet {i}"))?;
        sprite_paths.push(path);
    }

    if !json {
        eprintln!(
            "{} 雪碧图: {} 张, {}x{} 网格, {} 帧总计",
            "下载".cyan(),
            sprite_paths.len(),
            shot.img_x_len,
            shot.img_y_len,
            total_frames
        );
    }

    let ffmpeg = ffmpeg_available();
    let safe_title = sanitize(&info.title);
    let prefix = format!("{}_{}", id.label(), safe_title);

    let mut frames: Vec<ExtractedFrame> = Vec::new();
    for (idx, &ts) in timestamps.iter().enumerate() {
        // map timestamp to frame index
        let frame_idx = (ts * fps).round() as u32;
        let frame_idx = frame_idx.min(total_frames - 1);

        // which sprite sheet and local position
        let fps_per_sheet = shot.frames_per_sheet();
        let sheet = (frame_idx / fps_per_sheet) as usize;
        let local = frame_idx % fps_per_sheet;
        let col = local % shot.img_x_len;
        let row = local / shot.img_x_len;
        let x = col * shot.img_x_size;
        let y = row * shot.img_y_size;
        let w = shot.img_x_size;
        let h = shot.img_y_size;

        let out_file = out_dir.join(format!("{prefix}_{:03}.{format}", idx + 1));

        if ffmpeg {
            crop_with_ffmpeg(
                &sprite_paths[sheet.min(sprite_paths.len() - 1)],
                x,
                y,
                w,
                h,
                &out_file,
            )
            .await
            .with_context(|| format!("crop frame {idx}"))?;
        } else {
            // no ffmpeg: just copy the whole sprite sheet and save a note
            // (agent can crop later using the manifest coordinates)
            let fallback_file = out_dir.join(format!("{prefix}_{:03}_sprite{sheet}.{format}", idx + 1));
            tokio::fs::copy(&sprite_paths[sheet], &fallback_file).await?;
            if !json {
                eprintln!(
                    "{} 无 ffmpeg,保存完整雪碧图(需手动裁剪 x={x} y={y} w={w} h={h})",
                    "警告".yellow()
                );
            }
        }
        frames.push(ExtractedFrame {
            timestamp: ts,
            file: out_file,
        });
    }

    // cleanup temp sprites
    let _ = tokio::fs::remove_dir_all(&tmp).await;

    Ok(FrameResult {
        source: if ffmpeg {
            "storyboard".to_string()
        } else {
            "storyboard_no_ffmpeg".to_string()
        },
        frames,
    })
}

// ---------- ffmpeg path (download video, extract frames) ----------

async fn try_ffmpeg(
    bili: &Bili,
    id: &crate::bvid::VideoId,
    cid: u64,
    timestamps: &[f64],
    info: &crate::models::VideoInfo,
    out_dir: &Path,
    format: &str,
    quality: u32,
    json: bool,
) -> Result<FrameResult> {
    if !ffmpeg_available() {
        bail!("ffmpeg not found (required for --source=ffmpeg or storyboard fallback). Install with: brew install ffmpeg");
    }

    // get play url at requested quality (default 720P for readable text)
    let bundle = bili.play_url(id, cid, quality).await?;
    let dash = bundle
        .dash
        .as_ref()
        .ok_or_else(|| anyhow!("no DASH stream available for frame extraction"))?;

    // pick the stream matching requested quality, or best available
    let video = crate::commands::links::pick_video(dash, quality)?;
    let qn_desc = match video.id {
        127 => "8K", 126 => "杜比视界", 125 => "HDR", 120 => "4K",
        116 => "1080P60", 112 => "1080P高码", 100 => "1080P",
        74 => "720P60", 64 => "720P", 32 => "480P", 16 => "360P",
        _ => "未知",
    };

    let tmp = std::env::temp_dir().join(format!("bili-cli-frame-{}", cid));
    tokio::fs::create_dir_all(&tmp).await.ok();
    let v_path = tmp.join("video.mp4");

    if !json {
        eprintln!("{} 下载视频流 {} {}x{} ({}kbps) 用于截帧...", "ffmpeg".cyan(), qn_desc, video.width.unwrap_or(0), video.height.unwrap_or(0), video.bandwidth / 1000);
    }
    bili.download_to_file(&video.base_url, &v_path, None)
        .await
        .context("download video for frames")?;

    let safe_title = sanitize(&info.title);
    let prefix = format!("{}_{}", id.label(), safe_title);

    let mut frames: Vec<ExtractedFrame> = Vec::new();
    for (idx, &ts) in timestamps.iter().enumerate() {
        let out_file = out_dir.join(format!("{prefix}_{:03}.{format}", idx + 1));
        extract_frame_ffmpeg(&v_path, ts, &out_file)
            .await
            .with_context(|| format!("extract frame at {ts}s"))?;
        frames.push(ExtractedFrame {
            timestamp: ts,
            file: out_file,
        });
    }

    // cleanup
    let _ = tokio::fs::remove_dir_all(&tmp).await;

    Ok(FrameResult {
        source: "ffmpeg".to_string(),
        frames,
    })
}

// ---------- ffmpeg helpers ----------

fn ffmpeg_available() -> bool {
    std::process::Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

async fn crop_with_ffmpeg(
    sprite: &Path,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    out: &Path,
) -> Result<()> {
    let filter = format!("crop={w}:{h}:{x}:{y}");
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-i",
            sprite.to_str().unwrap(),
            "-vf",
            &filter,
            "-frames:v",
            "1",
            out.to_str().unwrap(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .context("run ffmpeg crop")?;
    if !status.success() {
        bail!("ffmpeg crop failed with status {status}");
    }
    Ok(())
}

async fn extract_frame_ffmpeg(video: &Path, ts: f64, out: &Path) -> Result<()> {
    let ts_str = format!("{ts:.3}");
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-ss",
            &ts_str,
            "-i",
            video.to_str().unwrap(),
            "-frames:v",
            "1",
            "-q:v",
            "2",
            out.to_str().unwrap(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .context("run ffmpeg extract")?;
    if !status.success() {
        bail!("ffmpeg extract failed at {ts}s with status {status}");
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
        let cut: String = trimmed.chars().take(60).collect();
        cut
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_by_count() {
        let ts = compute_timestamps(Some(4), None, None, 100.0).unwrap();
        // last frame clamped to duration - 0.5 to avoid end-of-video seek issues
        assert_eq!(ts.len(), 4);
        assert!((ts[0] - 0.0).abs() < 1e-6);
        assert!((ts[1] - 100.0 / 3.0).abs() < 1e-6);
        assert!((ts[3] - 99.5).abs() < 1e-6);
    }

    #[test]
    fn timestamps_default_count_is_8() {
        let ts = compute_timestamps(None, None, None, 80.0).unwrap();
        assert_eq!(ts.len(), 8);
        assert!((ts[0] - 0.0).abs() < 1e-6);
        // last frame clamped
        assert!((ts[7] - 79.5).abs() < 1e-6);
    }

    #[test]
    fn timestamps_by_interval() {
        let ts = compute_timestamps(None, Some(30.0), None, 100.0).unwrap();
        assert_eq!(ts, vec![0.0, 30.0, 60.0, 90.0]);
    }

    #[test]
    fn timestamps_by_at() {
        let ts = compute_timestamps(None, None, Some("10, 30, 60".to_string()), 100.0).unwrap();
        assert_eq!(ts, vec![10.0, 30.0, 60.0]);
    }

    #[test]
    fn timestamps_mutually_exclusive() {
        assert!(compute_timestamps(Some(4), Some(30.0), None, 100.0).is_err());
        assert!(compute_timestamps(Some(4), None, Some("10".to_string()), 100.0).is_err());
    }

    #[test]
    fn timestamps_single_count() {
        let ts = compute_timestamps(Some(1), None, None, 100.0).unwrap();
        assert_eq!(ts, vec![0.0]);
    }
}
