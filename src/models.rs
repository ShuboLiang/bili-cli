#![allow(dead_code, non_snake_case)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiResponse<T> {
    pub code: i64,
    #[serde(default)]
    pub message: String,
    pub data: Option<T>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VideoInfo {
    pub bvid: String,
    pub aid: u64,
    pub cid: u64,
    pub title: String,
    #[serde(default)]
    pub desc: String,
    #[serde(rename = "pubdate", default)]
    pub pubdate: u64,
    #[serde(default)]
    pub videos: u64,
    #[serde(default)]
    pub tid: u64,
    #[serde(default)]
    pub tname: String,
    pub owner: VideoOwner,
    pub stat: VideoStat,
    #[serde(default)]
    pub pic: String,
    #[serde(default)]
    pub duration: u64,
    #[serde(default)]
    pub dimension: Option<VideoDimension>,
    #[serde(default)]
    pub pages: Vec<VideoPage>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VideoOwner {
    pub mid: u64,
    pub name: String,
    #[serde(default)]
    pub face: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VideoStat {
    #[serde(default)]
    pub view: u64,
    #[serde(default)]
    pub danmaku: u64,
    #[serde(default)]
    pub reply: u64,
    #[serde(default)]
    pub favorite: u64,
    #[serde(default)]
    pub coin: u64,
    #[serde(default)]
    pub share: u64,
    #[serde(default)]
    pub like: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VideoDimension {
    pub width: u64,
    pub height: u64,
    #[serde(default)]
    pub rotate: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VideoPage {
    pub cid: u64,
    #[serde(default)]
    pub page: u64,
    #[serde(default)]
    pub part: String,
    #[serde(default)]
    pub duration: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlayUrlBundle {
    #[serde(default)]
    pub accept_quality: Vec<u32>,
    #[serde(default)]
    pub accept_description: Vec<String>,
    #[serde(default)]
    pub quality: u32,
    #[serde(default)]
    pub format: String,
    #[serde(default)]
    pub durl: Option<Vec<DurlItem>>,
    #[serde(default)]
    pub dash: Option<Dash>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DurlItem {
    pub order: u64,
    pub length: u64,
    pub size: u64,
    pub url: String,
    #[serde(default)]
    pub backup_url: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Dash {
    #[serde(default)]
    pub video: Vec<DashStream>,
    #[serde(default)]
    pub audio: Vec<DashStream>,
    #[serde(default)]
    pub dolby: Option<DashDolby>,
    #[serde(default)]
    pub flac: Option<DashFlac>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DashStream {
    pub id: u32,
    #[serde(default)]
    pub codecid: u32,
    #[serde(default)]
    pub codecs: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub backup_url: Option<Vec<String>>,
    #[serde(default)]
    pub bandwidth: u64,
    #[serde(default)]
    pub mime_type: String,
    #[serde(default)]
    pub width: Option<u64>,
    #[serde(default)]
    pub height: Option<u64>,
    #[serde(default)]
    pub frame_rate: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DashDolby {
    #[serde(default)]
    pub type_: Option<u32>,
    #[serde(default)]
    pub audio: Option<Vec<DashStream>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DashFlac {
    #[serde(default)]
    pub display: bool,
    #[serde(default)]
    pub audio: Option<DashStream>,
}

// ---------- Search ----------

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SearchResult {
    #[serde(default)]
    pub page: u64,
    #[serde(default)]
    pub pagesize: u64,
    #[serde(default)]
    pub numResults: u64,
    #[serde(default)]
    pub result: Vec<SearchItem>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SearchItem {
    #[serde(default)]
    pub aid: u64,
    #[serde(default)]
    pub bvid: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub mid: u64,
    #[serde(default)]
    pub play: u64,
    #[serde(default)]
    pub video_review: u64,
    #[serde(default)]
    pub review: u64,
    #[serde(default)]
    pub favorites: u64,
    #[serde(default)]
    pub tag: String,
    #[serde(default)]
    pub pic: String,
    #[serde(default)]
    pub duration: String,
    #[serde(default)]
    pub pubdate: u64,
}

// ---------- Subtitle ----------

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlayerView {
    #[serde(default)]
    pub subtitle: Option<PlayerSubtitle>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlayerSubtitle {
    #[serde(default)]
    pub subtitles: Vec<SubtitleMeta>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SubtitleMeta {
    #[serde(default)]
    pub lan: String,
    #[serde(default)]
    pub lan_doc: String,
    #[serde(default)]
    pub subtitle_url: String,
    #[serde(default)]
    pub ai_type: u32,
    #[serde(default)]
    pub ai_status: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SubtitleBody {
    #[serde(default)]
    pub body: Vec<SubtitleLine>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SubtitleLine {
    pub from: f64,
    pub to: f64,
    #[serde(default)]
    pub content: String,
}

impl SubtitleLine {
    pub fn start_ms(&self) -> u128 {
        (self.from.max(0.0) * 1000.0) as u128
    }
    pub fn end_ms(&self) -> u128 {
        (self.to.max(0.0) * 1000.0) as u128
    }
}

// ---------- Videoshot (storyboard / sprite sheet) ----------

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Videoshot {
    #[serde(default)]
    pub pvdata: String,
    #[serde(default)]
    pub img_x_len: u32, // columns per sheet
    #[serde(default)]
    pub img_y_len: u32, // rows per sheet
    #[serde(default)]
    pub img_x_size: u32, // frame width (px)
    #[serde(default)]
    pub img_y_size: u32, // frame height (px)
    #[serde(default)]
    pub image: Vec<String>, // sprite sheet URLs (may be multiple)
    #[serde(default)]
    pub index: Vec<u32>, // per-frame timestamps in seconds (usually empty)
}

impl Videoshot {
    pub fn frames_per_sheet(&self) -> u32 {
        self.img_x_len * self.img_y_len
    }
    pub fn total_frames(&self) -> u32 {
        self.frames_per_sheet() * self.image.len() as u32
    }
}
