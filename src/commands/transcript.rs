use anyhow::Result;
use serde::Serialize;
use std::io::Write;
use std::path::PathBuf;

use crate::api::Bili;
use crate::commands::{cid_for_page, pick_best_subtitle, resolve};
use crate::models::{SubtitleLine, SubtitleMeta, VideoInfo};

pub async fn run(
    bili: &Bili,
    raw: &str,
    page: usize,
    start: f64,
    end: f64,
    max_chars: usize,
    format: &str,
    no_timestamps: bool,
    out: Option<PathBuf>,
    json: bool,
) -> Result<()> {
    let (id, info) = resolve(bili, raw).await?;
    let cid = cid_for_page(&info, page);

    let view = bili.player_view(&id, cid).await?;
    let subs = view
        .subtitle
        .as_ref()
        .map(|s| &s.subtitles)
        .filter(|v| !v.is_empty());

    match subs {
        None => handle_no_subtitle(&info, page, cid, &id.label(), json, out, format),
        Some(list) if list.is_empty() => {
            handle_no_subtitle(&info, page, cid, &id.label(), json, out, format)
        }
        Some(list) => {
            let chosen = pick_best_subtitle(list);
            if chosen.subtitle_url.is_empty() {
                return handle_no_subtitle(&info, page, cid, &id.label(), json, out, format);
            }
            let body = bili.fetch_subtitle(&chosen.subtitle_url).await?;
            if body.body.is_empty() {
                return handle_no_subtitle(&info, page, cid, &id.label(), json, out, format);
            }

            let paragraphs = aggregate(&body.body, start, end);
            let payload = build_payload(&info, page, cid, chosen, &paragraphs);

            if json {
                let mut final_payload = payload.clone();
                let (body_text, truncated, char_count) =
                    render_body(&payload, format, no_timestamps, max_chars);
                final_payload.truncated = truncated;
                final_payload.char_count = char_count;
                final_payload.body = Some(body_text);
                return emit(&final_payload, out, true);
            }

            let (body_text, truncated, _char_count) =
                render_body(&payload, format, no_timestamps, max_chars);
            let mut out_text = body_text;
            if truncated {
                out_text.push_str("\n\n[已截断:超过 --max-chars ");
                out_text.push_str(&max_chars.to_string());
                out_text.push_str(" 字符]");
            }
            emit_text(&out_text, out)
        }
    }
}

fn handle_no_subtitle(
    info: &VideoInfo,
    page: usize,
    cid: u64,
    id_label: &str,
    json: bool,
    out: Option<PathBuf>,
    _format: &str,
) -> Result<()> {
    let fallback = Fallback {
        strategy: "audio_asr".to_string(),
        reason: "no subtitle found for this video".to_string(),
        steps: vec![
            format!("bili-cli download {id_label} --audio-only -o /tmp/"),
            "upload the .m4a to a speech-to-text service (e.g. 飞书妙记 / lark-minutes skill) to get a transcript".to_string(),
            "feed the transcript to an LLM for summarization".to_string(),
        ],
    };

    if json {
        let payload = TranscriptJson {
            video: video_ref(info),
            page: page_ref(info, page, cid),
            subtitle_available: false,
            subtitle: None,
            paragraphs: None,
            body: None,
            fallback: Some(fallback),
            truncated: false,
            char_count: 0,
        };
        return emit(&payload, out, true);
    }

    let text = format!(
        "此视频(P{} cid={})没有可用字幕,无法直接生成逐字稿。\n\
         要让 agent 转写音频,按以下步骤:\n  1. {step1}\n  2. {step2}\n  3. {step3}\n",
        page.max(1),
        cid,
        step1 = fallback.steps[0],
        step2 = fallback.steps[1],
        step3 = fallback.steps[2],
    );
    emit_text(&text, out)
}

// ---------- payload types ----------

#[derive(Serialize, Clone)]
struct TranscriptJson {
    video: VideoRef,
    page: PageRef,
    subtitle_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    subtitle: Option<SubtitleRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    paragraphs: Option<Vec<ParaRef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fallback: Option<Fallback>,
    truncated: bool,
    char_count: usize,
}

#[derive(Serialize, Clone)]
struct VideoRef {
    bvid: String,
    aid: u64,
    title: String,
    owner: String,
    duration: u64,
}

#[derive(Serialize, Clone)]
struct PageRef {
    page: u64,
    cid: u64,
    part: String,
}

#[derive(Serialize, Clone)]
struct SubtitleRef {
    lan: String,
    lan_doc: String,
    ai: bool,
}

#[derive(Serialize, Clone)]
struct ParaRef {
    start: f64,
    end: f64,
    text: String,
}

#[derive(Serialize, Clone)]
struct Fallback {
    strategy: String,
    reason: String,
    steps: Vec<String>,
}

fn video_ref(info: &VideoInfo) -> VideoRef {
    VideoRef {
        bvid: info.bvid.clone(),
        aid: info.aid,
        title: info.title.clone(),
        owner: info.owner.name.clone(),
        duration: info.duration,
    }
}

fn page_ref(info: &VideoInfo, page: usize, cid: u64) -> PageRef {
    let idx = if page == 0 { 0 } else { page - 1 };
    let p = info.pages.get(idx);
    PageRef {
        page: p.map(|x| x.page).unwrap_or(page.max(1) as u64),
        cid,
        part: p.map(|x| x.part.clone()).unwrap_or_default(),
    }
}

fn build_payload(
    info: &VideoInfo,
    page: usize,
    cid: u64,
    chosen: &SubtitleMeta,
    paragraphs: &[Paragraph],
) -> TranscriptJson {
    TranscriptJson {
        video: video_ref(info),
        page: page_ref(info, page, cid),
        subtitle_available: true,
        subtitle: Some(SubtitleRef {
            lan: chosen.lan.clone(),
            lan_doc: chosen.lan_doc.clone(),
            ai: chosen.ai_type != 0,
        }),
        paragraphs: Some(
            paragraphs
                .iter()
                .map(|p| ParaRef {
                    start: p.start,
                    end: p.end,
                    text: p.text.clone(),
                })
                .collect(),
        ),
        body: None,
        fallback: None,
        truncated: false,
        char_count: 0,
    }
}

// ---------- aggregation ----------

#[derive(Debug, Clone)]
struct Paragraph {
    start: f64,
    end: f64,
    text: String,
}

/// Merge raw subtitle lines into reader-friendly paragraphs.
/// Break on: gap > 2s, accumulated duration > 15s, or sentence-ending punctuation.
fn aggregate(lines: &[SubtitleLine], start: f64, end: f64) -> Vec<Paragraph> {
    let mut paras: Vec<Paragraph> = Vec::new();
    let mut cur_text = String::new();
    let mut cur_start = 0.0f64;
    let mut cur_end = 0.0f64;
    let mut last_to = -1.0f64;

    for line in lines {
        // range filter
        if end > 0.0 && line.from >= end {
            break;
        }
        if line.to <= start {
            continue;
        }

        let content = line.content.trim();
        if content.is_empty() {
            continue;
        }

        let gap = if last_to < 0.0 { 0.0 } else { line.from - last_to };
        let dur = cur_end - cur_start;
        let ends_with_punct = matches!(
            cur_text.chars().last(),
            Some('。' | '！' | '？' | '!' | '?' | ';' | '；' | '.' | '…')
        );

        let should_break = !cur_text.is_empty()
            && (gap > 2.0 || dur > 15.0 || ends_with_punct);

        if should_break {
            paras.push(Paragraph {
                start: cur_start,
                end: cur_end,
                text: cur_text.trim().to_string(),
            });
            cur_text.clear();
        }
        if cur_text.is_empty() {
            cur_start = line.from.max(start);
        }
        push_with_space(&mut cur_text, content);
        cur_end = if end > 0.0 { line.to.min(end) } else { line.to };
        last_to = line.to;
    }
    if !cur_text.trim().is_empty() {
        paras.push(Paragraph {
            start: cur_start,
            end: cur_end,
            text: cur_text.trim().to_string(),
        });
    }
    paras
}

/// Append `s` to `buf`, inserting a space only when neither the last char of
/// `buf` nor the first char of `s` is CJK (Chinese/Japanese/Korean).
fn push_with_space(buf: &mut String, s: &str) {
    let p = buf.chars().last();
    let n = s.chars().next();
    let cjk = |c: char| -> bool {
        let u = c as u32;
        (0x4E00..=0x9FFF).contains(&u)
            || (0x3400..=0x4DBF).contains(&u)
            || (0x3000..=0x303F).contains(&u)
            || (0xFF00..=0xFFEF).contains(&u)
    };
    let need_space = match (p, n) {
        (Some(p), Some(n)) => !cjk(p) && !cjk(n) && !p.is_whitespace() && !n.is_whitespace(),
        _ => false,
    };
    if need_space {
        buf.push(' ');
    }
    buf.push_str(s);
}

// ---------- rendering ----------

fn fmt_ts(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    let h = s / 3600;
    let m = (s % 3600) / 60;
    let sec = s % 60;
    if h > 0 {
        format!("{}:{:02}:{:02}", h, m, sec)
    } else {
        format!("{:02}:{:02}", m, sec)
    }
}

/// Render the readable body (text or markdown) from paragraphs, applying
/// `max_chars` truncation. Returns (body, truncated, char_count).
fn render_body(
    payload: &TranscriptJson,
    format: &str,
    no_timestamps: bool,
    max_chars: usize,
) -> (String, bool, usize) {
    let paras = match &payload.paragraphs {
        Some(p) => p,
        None => return (String::new(), false, 0),
    };
    let md = format.eq_ignore_ascii_case("md") || format.eq_ignore_ascii_case("markdown");

    let mut out = String::new();
    for p in paras {
        let line = if no_timestamps {
            p.text.clone()
        } else if md {
            format!("## [{}]  \n{}\n", fmt_ts(p.start), p.text)
        } else {
            format!("[{}] {}", fmt_ts(p.start), p.text)
        };
        if max_chars > 0 && out.chars().count() + line.chars().count() > max_chars {
            let remaining = max_chars.saturating_sub(out.chars().count());
            if remaining > 0 {
                let take: String = line.chars().take(remaining).collect();
                out.push_str(&take);
            }
            out.push('…');
            let cc = out.chars().count();
            return (out, true, cc);
        }
        out.push_str(&line);
        out.push_str("\n\n");
    }
    let cc = out.chars().count();
    (out, false, cc)
}

// ---------- emit ----------

fn emit<T: Serialize>(payload: &T, out: Option<PathBuf>, _json: bool) -> Result<()> {
    let s = serde_json::to_string_pretty(payload)?;
    match out {
        Some(p) => {
            let mut path = p;
            if path.extension().is_none() {
                path = path.with_extension("json");
            }
            std::fs::write(&path, s + "\n")?;
            eprintln!("已保存: {}", path.display());
        }
        None => println!("{s}"),
    }
    Ok(())
}

fn emit_text(text: &str, out: Option<PathBuf>) -> Result<()> {
    match out {
        Some(p) => {
            let mut f = std::fs::File::create(&p)?;
            f.write_all(text.as_bytes())?;
            eprintln!("已保存: {}", p.display());
        }
        None => {
            print!("{text}");
            use std::io::Write as _;
            std::io::stdout().flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::SubtitleLine;

    fn line(from: f64, to: f64, content: &str) -> SubtitleLine {
        SubtitleLine {
            from,
            to,
            content: content.to_string(),
        }
    }

    #[test]
    fn aggregate_merges_consecutive_lines() {
        let lines = vec![
            line(0.0, 1.0, "你好"),
            line(1.0, 2.0, "世界"),
            line(2.0, 3.0, "今天"),
        ];
        let paras = aggregate(&lines, 0.0, 0.0);
        assert_eq!(paras.len(), 1);
        assert_eq!(paras[0].text, "你好世界今天");
        assert!((paras[0].start - 0.0).abs() < 1e-6);
        assert!((paras[0].end - 3.0).abs() < 1e-6);
    }

    #[test]
    fn aggregate_breaks_on_gap() {
        let lines = vec![line(0.0, 1.0, "第一段"), line(5.0, 6.0, "第二段")];
        let paras = aggregate(&lines, 0.0, 0.0);
        assert_eq!(paras.len(), 2);
        assert_eq!(paras[0].text, "第一段");
        assert_eq!(paras[1].text, "第二段");
    }

    #[test]
    fn aggregate_breaks_on_punctuation() {
        let lines = vec![line(0.0, 1.0, "句号结束。"), line(1.2, 2.0, "新句子")];
        let paras = aggregate(&lines, 0.0, 0.0);
        assert_eq!(paras.len(), 2);
        assert_eq!(paras[0].text, "句号结束。");
        assert_eq!(paras[1].text, "新句子");
    }

    #[test]
    fn aggregate_respects_range() {
        let lines = vec![
            line(0.0, 1.0, "前面"),
            line(5.0, 6.0, "中间"),
            line(10.0, 11.0, "后面"),
        ];
        let paras = aggregate(&lines, 4.0, 8.0);
        assert_eq!(paras.len(), 1);
        assert!(paras[0].text.contains("中间"));
    }

    #[test]
    fn aggregate_cjk_no_extra_space() {
        let lines = vec![line(0.0, 1.0, "你好"), line(1.0, 2.0, "世界")];
        let paras = aggregate(&lines, 0.0, 0.0);
        assert_eq!(paras[0].text, "你好世界");
    }

    #[test]
    fn aggregate_english_adds_space() {
        let lines = vec![line(0.0, 1.0, "hello"), line(1.0, 2.0, "world")];
        let paras = aggregate(&lines, 0.0, 0.0);
        assert_eq!(paras[0].text, "hello world");
    }

    #[test]
    fn fmt_ts_minutes_seconds() {
        assert_eq!(fmt_ts(0.0), "00:00");
        assert_eq!(fmt_ts(65.4), "01:05");
        assert_eq!(fmt_ts(3725.0), "1:02:05");
    }
}
