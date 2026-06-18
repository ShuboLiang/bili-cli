use anyhow::Result;
use colored::Colorize;

use crate::api::Bili;
use crate::commands::{header, human_count, human_duration, resolve};

pub async fn run(bili: &Bili, raw: &str, json: bool) -> Result<()> {
    let (id, info) = resolve(bili, raw).await?;

    if json {
        return crate::commands::print_json(&info);
    }

    println!("{}", info.title.bold());
    println!("{} {}", header("ID:    "), id.label());
    println!("{} {} (avid {})", header("UP:    "), info.owner.name, info.aid);
    if !info.tname.is_empty() {
        println!("{} {}", header("分区:  "), info.tname);
    }
    if !info.desc.trim().is_empty() {
        let desc = info.desc.trim();
        let clipped = if desc.chars().count() > 200 {
            let cut: String = desc.chars().take(200).collect();
            format!("{cut}…")
        } else {
            desc.to_string()
        };
        println!("{} {}", header("简介:  "), clipped);
    }

    println!("\n{}", "统计数据".bold().cyan());
    let s = &info.stat;
    println!(
        "  播放 {}  弹幕 {}  点赞 {}  投币 {}  收藏 {}  转发 {}  评论 {}",
        human_count(s.view).yellow(),
        human_count(s.danmaku),
        human_count(s.like).green(),
        human_count(s.coin).yellow(),
        human_count(s.favorite).yellow(),
        human_count(s.share),
        human_count(s.reply),
    );

    if let Some(d) = &info.dimension {
        if d.height > 0 {
            println!(
                "\n{} {}x{}  时长 {}  分P {}",
                header("规格:"),
                d.width,
                d.height,
                human_duration(info.duration),
                info.videos
            );
        }
    } else {
        println!(
            "\n{} 时长 {}  分P {}",
            header("规格:"),
            human_duration(info.duration),
            info.videos
        );
    }

    if !info.pic.is_empty() {
        println!("\n{} {}", header("封面:  "), info.pic);
    }

    if !info.pages.is_empty() && info.pages.len() > 1 {
        println!("\n{}", "分P 列表".bold().cyan());
        for p in &info.pages {
            println!(
                "  P{:>2}  cid={}  {}  ({})",
                p.page.max(1),
                p.cid,
                if p.part.is_empty() { "(未命名)" } else { &p.part },
                human_duration(p.duration),
            );
        }
    }

    Ok(())
}
