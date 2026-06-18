use anyhow::Result;
use colored::Colorize;
use serde::Serialize;
use tabled::{Table, Tabled};

use crate::api::Bili;
use crate::commands::{human_count, human_duration};

#[derive(Tabled)]
struct Row {
    #[tabled(rename = "#")]
    idx: usize,
    #[tabled(rename = "BV")]
    bvid: String,
    #[tabled(rename = "标题")]
    title: String,
    #[tabled(rename = "UP主")]
    author: String,
    #[tabled(rename = "播放")]
    play: String,
    #[tabled(rename = "时长")]
    duration: String,
}

#[derive(Serialize)]
struct SearchResultJson {
    keyword: String,
    count: usize,
    results: Vec<SearchItemJson>,
}

#[derive(Serialize)]
struct SearchItemJson {
    bvid: String,
    aid: u64,
    title: String,
    author: String,
    play: u64,
    duration: String,
    pubdate: u64,
    description: String,
    pic: String,
}

fn strip_tags(s: &str) -> String {
    // search results include <em class="keyword">...</em> highlights
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

pub async fn run(bili: &Bili, keyword: &str, limit: usize, json: bool) -> Result<()> {
    if bili.sessdata.is_none() && !json {
        eprintln!("{}", "提示: 未提供 SESSDATA,搜索可能返回空结果。用 --cookie 或 BILI_SESSDATA 环境变量设置。".yellow());
    }

    let pagesize = limit.min(50) as u32;
    let mut page: u32 = 1;
    let mut rows: Vec<Row> = Vec::new();
    let mut items: Vec<SearchItemJson> = Vec::new();
    let mut idx = 1usize;

    loop {
        let res = bili.search(keyword, page, pagesize).await?;
        if res.result.is_empty() {
            break;
        }
        let got = res.result.len();
        for item in &res.result {
            if idx > limit {
                break;
            }
            // some non-video entries can sneak in; skip if no bvid
            if item.bvid.is_empty() {
                continue;
            }
            let clean_title = strip_tags(&item.title);
            if json {
                items.push(SearchItemJson {
                    bvid: item.bvid.clone(),
                    aid: item.aid,
                    title: clean_title.clone(),
                    author: item.author.clone(),
                    play: item.play,
                    duration: item.duration.clone(),
                    pubdate: item.pubdate,
                    description: item.description.clone(),
                    pic: item.pic.clone(),
                });
            } else {
                let title = if clean_title.chars().count() > 48 {
                    let cut: String = clean_title.chars().take(48).collect();
                    format!("{cut}…")
                } else {
                    clean_title
                };
                rows.push(Row {
                    idx,
                    bvid: item.bvid.clone(),
                    title,
                    author: item.author.clone(),
                    play: human_count(item.play),
                    duration: human_duration(parse_mmss(&item.duration)),
                });
            }
            idx += 1;
        }
        if idx > limit || got < pagesize as usize {
            break;
        }
        page += 1;
        if page > 5 {
            break;
        }
    }

    if json {
        let payload = SearchResultJson {
            keyword: keyword.to_string(),
            count: items.len(),
            results: items,
        };
        return crate::commands::print_json(&payload);
    }

    if rows.is_empty() {
        eprintln!("{}", "没有匹配结果".yellow());
        return Ok(());
    }

    let table = Table::new(rows);
    println!("{table}");
    Ok(())
}

fn parse_mmss(s: &str) -> u64 {
    // duration is like "12:34" or "1:02:03"
    let parts: Vec<u64> = s.split(':').filter_map(|p| p.parse().ok()).collect();
    match parts.len() {
        3 => parts[0] * 3600 + parts[1] * 60 + parts[2],
        2 => parts[0] * 60 + parts[1],
        1 => parts[0],
        _ => 0,
    }
}
