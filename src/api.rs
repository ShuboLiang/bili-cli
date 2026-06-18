use anyhow::{anyhow, bail, Context, Result};
use reqwest::{Client, ClientBuilder};
use serde::de::DeserializeOwned;
use std::time::Duration;

use crate::bvid::VideoId;
use crate::models::*;

const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
    AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";
const REFERER: &str = "https://www.bilibili.com/";

pub struct Bili {
    pub client: Client,
    pub sessdata: Option<String>,
}

impl Bili {
    pub fn new(sessdata: Option<String>) -> Result<Self> {
        let mut builder = ClientBuilder::new()
            .user_agent(UA)
            .cookie_store(true)
            .timeout(Duration::from_secs(30));

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::REFERER,
            reqwest::header::HeaderValue::from_static(REFERER),
        );

        if let Some(ref s) = sessdata {
            let v = format!("SESSDATA={}", s);
            let cookie = reqwest::header::HeaderValue::from_str(&v)
                .context("invalid SESSDATA cookie value")?;
            headers.insert(reqwest::header::COOKIE, cookie);
        }

        builder = builder.default_headers(headers);
        let client = builder.build()?;
        Ok(Self { client, sessdata })
    }

    async fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .with_context(|| format!("request failed: {url}"))?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!("HTTP {status} for {url}: {text}");
        }
        let body: ApiResponse<T> = resp.json().await.context("decode response")?;
        if body.code != 0 {
            bail!(
                "bilibili api error {}: {}",
                body.code,
                body.message
            );
        }
        body.data
            .ok_or_else(|| anyhow!("bilibili api returned empty data (code 0)"))
    }

    pub async fn video_info(&self, id: &VideoId) -> Result<VideoInfo> {
        let (k, v) = id.as_query_pair();
        let url = format!("https://api.bilibili.com/x/web-interface/view?{k}={v}");
        self.get_json(&url).await
    }

    pub async fn play_url(&self, id: &VideoId, cid: u64, qn: u32) -> Result<PlayUrlBundle> {
        let (k, v) = id.as_query_pair();
        // fnval=16 -> DASH; 4048 includes 8K/HDR/Dolby/AV1 flags.
        let url = format!(
            "https://api.bilibili.com/x/player/playurl?{k}={v}&cid={cid}&qn={qn}&fnval=4048&fnver=0&fourk=1"
        );
        self.get_json(&url).await
    }

    pub async fn search(&self, keyword: &str, page: u32, pagesize: u32) -> Result<SearchResult> {
        // search needs the search-style endpoint and proper headers; for simplicity use the
        // x/web-interface/search/type endpoint.
        let url = format!(
            "https://api.bilibili.com/x/web-interface/search/type?search_type=video&keyword={kw}&page={page}&pagesize={ps}&order=pubdate",
            kw = urlencoding::encode(keyword),
            page = page,
            ps = pagesize,
        );
        self.get_json(&url).await
    }

    pub async fn player_view(&self, id: &VideoId, cid: u64) -> Result<PlayerView> {
        let (k, v) = id.as_query_pair();
        let url = format!(
            "https://api.bilibili.com/x/player/v2?{k}={v}&cid={cid}"
        );
        self.get_json(&url).await
    }

    pub async fn fetch_subtitle(&self, url: &str) -> Result<SubtitleBody> {
        // subtitle_url may be protocol-relative (//ais...)
        let full = if url.starts_with("//") {
            format!("https:{url}")
        } else if url.starts_with("http://") || url.starts_with("https://") {
            url.to_string()
        } else {
            format!("https://{url}")
        };
        let resp = self.client.get(&full).send().await?;
        let status = resp.status();
        if !status.is_success() {
            bail!("subtitle fetch failed: HTTP {status}");
        }
        resp.json::<SubtitleBody>()
            .await
            .context("decode subtitle body")
    }

    pub async fn download_to_file(
        &self,
        url: &str,
        out_path: &std::path::Path,
        bar: Option<&indicatif::ProgressBar>,
    ) -> Result<()> {
        use tokio::io::AsyncWriteExt;

        let mut resp = self.client.get(url).send().await?;
        let status = resp.status();
        if !status.is_success() {
            bail!("download failed: HTTP {status}");
        }
        let total = resp.content_length();
        if let Some(b) = bar {
            if let Some(t) = total {
                b.set_length(t);
            }
        }
        let mut file = tokio::fs::File::create(out_path).await?;
        while let Some(chunk) = resp.chunk().await? {
            file.write_all(&chunk).await?;
            if let Some(b) = bar {
                b.inc(chunk.len() as u64);
            }
        }
        file.flush().await?;
        Ok(())
    }
}

// tiny inline url-encoding to avoid an extra dep; mirrors `urlencoding::encode`
mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut out = String::with_capacity(s.len() * 3);
        for &b in s.as_bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                    out.push(b as char);
                }
                _ => out.push_str(&format!("%{:02X}", b)),
            }
        }
        out
    }
}
