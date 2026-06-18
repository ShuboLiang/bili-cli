use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "bili-cli",
    version,
    about = "A CLI tool for parsing, searching, downloading Bilibili videos and extracting subtitles",
    long_about = None
)]
pub struct Cli {
    /// Bilibili SESSDATA cookie value (or set BILI_SESSDATA env var).
    /// Needed for higher-quality streams, search and some subtitles.
    #[arg(long, short = 'c', global = true, env = "BILI_SESSDATA")]
    pub sessdata: Option<String>,

    /// Machine-readable JSON output for all commands (for agents / scripting).
    /// Suppresses colored tables and progress bars on stdout.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Parse and display video metadata (title, author, stats, cover, ...).
    Info {
        /// BV id (BV1xx...), AV id (av123 / 123), or full video URL.
        id: String,
    },
    /// Search videos by keyword.
    Search {
        /// Search keyword.
        keyword: String,
        /// Maximum number of results to show.
        #[arg(long, short = 'n', default_value_t = 20)]
        limit: usize,
    },
    /// Extract playable stream URLs (video/audio, by quality).
    Links {
        /// BV id, AV id, or full video URL.
        id: String,
        /// Preferred quality (qn). 127=8K,126=Dolby Vision,125=HDR,120=4K,
        /// 116=1080P60,112=1080P+,100=1080P,74=720P60,64=720P,32=480P,16=360P.
        /// Use 0 (default) to request the highest available.
        #[arg(long, short = 'q', default_value_t = 0)]
        quality: u32,
        /// Only print URLs (no table), one per line.
        #[arg(long)]
        raw: bool,
    },
    /// Download a video (and optionally merge with audio via ffmpeg).
    Download {
        /// BV id, AV id, or full video URL.
        id: String,
        /// Output directory (default: current dir).
        #[arg(long, short = 'o', default_value = ".")]
        out_dir: PathBuf,
        /// Preferred quality (qn). See `links` help for the table.
        #[arg(long, short = 'q', default_value_t = 0)]
        quality: u32,
        /// Download audio only (best quality audio stream).
        #[arg(long)]
        audio_only: bool,
        /// Skip merging with ffmpeg; save raw video/audio streams separately.
        #[arg(long)]
        no_merge: bool,
        /// Pick a specific page (分P) by 1-based index. Default: 1.
        #[arg(long, short = 'p', default_value_t = 1)]
        page: usize,
    },
    /// Intelligently extract subtitles / captions for a video.
    Subtitle {
        /// BV id, AV id, or full video URL.
        id: String,
        /// Pick a specific page (分P) by 1-based index. Default: 1.
        #[arg(long, short = 'p', default_value_t = 1)]
        page: usize,
        /// Output file. If omitted, prints to stdout.
        #[arg(long, short = 'o')]
        out: Option<PathBuf>,
        /// Output format: srt, vtt, json, txt.
        #[arg(long, short = 'f', default_value = "srt")]
        format: String,
        /// Pick a specific subtitle by 1-based index (use `list` to see options).
        /// 0 (default) = auto pick: prefer zh-Hans > zh > ai-zh > any.
        #[arg(long, short = 'i', default_value_t = 0)]
        index: usize,
        /// Just list available subtitles, do not extract.
        #[arg(long)]
        list: bool,
    },
    /// Extract key frames (screenshots) at specified timestamps.
    /// Prefers Bilibili storyboard (sprite sheet, fast); falls back to ffmpeg.
    /// Output images + a JSON manifest for LLM vision analysis.
    Frames {
        /// BV id, AV id, or full video URL.
        id: String,
        /// Output directory for frame images.
        #[arg(long, short = 'o', default_value = ".")]
        out_dir: PathBuf,
        /// Number of evenly-spaced frames to extract.
        /// Mutually exclusive with --interval and --at. Default: 8.
        #[arg(long, short = 'n')]
        count: Option<usize>,
        /// Extract one frame every N seconds.
        /// Mutually exclusive with --count and --at.
        #[arg(long)]
        interval: Option<f64>,
        /// Extract frames at specific timestamps (seconds), comma-separated.
        /// Mutually exclusive with --count and --interval.
        /// Example: --at 30,120,300
        #[arg(long)]
        at: Option<String>,
        /// Frame source: auto (storyboard first, then ffmpeg), storyboard, ffmpeg.
        /// Use ffmpeg for high-res frames (storyboard is only ~480x270 thumbnails).
        #[arg(long, default_value = "auto")]
        source: String,
        /// Frame image format: jpg (smaller, default) or png.
        #[arg(long, default_value = "jpg")]
        format: String,
        /// Video quality for ffmpeg frame extraction.
        /// 16=360P, 32=480P, 64=720P, 80=1080P, 112=1080P高码率, 116=1080P60.
        /// Only affects ffmpeg path; storyboard is always low-res.
        /// Default: 64 (720P) — readable for most slides/text.
        #[arg(long, short = 'q', default_value_t = 64)]
        quality: u32,
        /// Pick a specific page (分P) by 1-based index. Default: 1.
        #[arg(long, short = 'p', default_value_t = 1)]
        page: usize,
    },
    /// Build an LLM-friendly transcript (paragraph-aggregated, timestamped)
    /// from the best available subtitle. Falls back to a clear hint when no
    /// subtitle exists (so agents can switch to audio transcription).
    Transcript {
        /// BV id, AV id, or full video URL.
        id: String,
        /// Pick a specific page (分P) by 1-based index. Default: 1 (first page).
        #[arg(long, short = 'p', default_value_t = 1)]
        page: usize,
        /// Start time in seconds (crop transcript from this point).
        #[arg(long, default_value_t = 0.0)]
        start: f64,
        /// End time in seconds (crop transcript up to this point). 0 = until end.
        #[arg(long, default_value_t = 0.0)]
        end: f64,
        /// Hard cap on output characters (token-budget guard). 0 = no cap.
        /// When truncated, a note is appended (text/md) or `truncated=true` (json).
        #[arg(long, default_value_t = 0)]
        max_chars: usize,
        /// Transcript body format: text (default) or markdown.
        /// Ignored when --json is set (use --format to pick the embedded body style).
        #[arg(long, short = 'f', default_value = "text")]
        format: String,
        /// Omit leading timestamps from each paragraph.
        #[arg(long)]
        no_timestamps: bool,
        /// Output file. If omitted, prints to stdout.
        #[arg(long, short = 'o')]
        out: Option<PathBuf>,
    },
}
