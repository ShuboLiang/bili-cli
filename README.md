# bili-cli

一个用 Rust 编写的 Bilibili 命令行工具：解析视频信息、搜索视频、提取下载链接、下载视频，生成 LLM 友好的逐字稿。专为 agent / LLM 编排设计，所有命令支持 `--json` 机器可读输出。

## 功能

- `info` —— 解析视频元数据（标题、UP 主、播放/点赞/收藏统计、封面、分 P 列表等）
- `search` —— 按关键词搜索视频，表格化展示结果
- `links` —— 提取可播放的 DASH 流地址（视频/音频，按清晰度 `qn` 和编码分类）
- `download` —— 下载视频与音频流，并自动调用 `ffmpeg` 合并为 mp4（支持仅音频、跳过合并）
- `subtitle` —— 智能提取字幕：自动优先选择 `zh-Hans` > `zh-Hant` > `zh`（人工）> `AI` > `en`，输出 `srt` / `vtt` / `json` / `txt`
- `transcript` —— 生成 LLM 友好的逐字稿：将字幕聚合成带时间戳的段落，支持范围裁剪与字符上限。**无字幕时输出结构化降级提示**（引导 agent 走音频转写流程）
- 全局 `--json` —— 所有命令输出机器可读 JSON，供 agent / 脚本可靠解析

## 安装

```bash
git clone https://github.com/ShuboLiang/bili-cli.git
cd bili-cli
cargo install --path .
```

> 下载合并功能需要本机已安装 `ffmpeg`（在 `PATH` 中可用）。macOS：`brew install ffmpeg`。

## 使用

```bash
# 解析视频信息（支持 BV 号、av 号、完整 URL）
bili-cli info BV1xx411x7xx
bili-cli info https://www.bilibili.com/video/BV1xx411x7xx

# 搜索视频
bili-cli search "rust 语言" -n 15

# 提取下载链接
bili-cli links BV1xx411x7xx
bili-cli links BV1xx411x7xx -q 120      # 指定清晰度（4K）
bili-cli links BV1xx411x7xx --raw        # 仅打印 URL

# 下载视频（自动合并）
bili-cli download BV1xx411x7xx -o ./downloads
bili-cli download BV1xx411x7xx --audio-only        # 仅音频
bili-cli download BV1xx411x7xx --no-merge          # 不合并，分别保存

# 字幕
bili-cli subtitle BV1xx411x7xx                     # 智能选最佳字幕，输出 srt
bili-cli subtitle BV1xx411x7xx --list              # 列出可用字幕
bili-cli subtitle BV1xx411x7xx -f vtt -o out.vtt   # 输出 vtt 到文件
bili-cli subtitle BV1xx411x7xx -i 2                # 按索引选择第 2 个字幕

# 逐字稿（LLM 友好，专为 agent 总结设计）
bili-cli transcript BV1xx411x7xx                        # 输出带时间戳的段落逐字稿
bili-cli transcript BV1xx411x7xx --max-chars 8000       # 限制字符数（token 预算）
bili-cli transcript BV1xx411x7xx --start 60 --end 600   # 只取 1~10 分钟
bili-cli transcript BV1xx411x7xx -f markdown            # markdown 格式
bili-cli transcript BV1xx411x7xx --no-timestamps        # 不带时间戳

# 机器可读 JSON（agent / 脚本）
bili-cli --json info BV1xx411x7xx          # 视频元数据 JSON
bili-cli --json transcript BV1xx411x7xx    # 逐字稿 JSON（含 fallback 降级提示）
```

## 给 Agent / LLM 用

`--json` 模式下，`transcript` 在无字幕时返回结构化降级指引，agent 可据此自动切换到音频转写流程：

```json
{
  "video": { "bvid": "...", "title": "...", "owner": "...", "duration": 213 },
  "page": { "page": 1, "cid": 137649199, "part": "..." },
  "subtitle_available": false,
  "fallback": {
    "strategy": "audio_asr",
    "reason": "no subtitle found for this video",
    "steps": [
      "bili-cli download <id> --audio-only -o /tmp/",
      "upload the .m4a to a speech-to-text service (e.g. 飞书妙记 / lark-minutes skill)",
      "feed the transcript to an LLM for summarization"
    ]
  }
}
```

有字幕时返回 `{video, page, subtitle, paragraphs:[{start,end,text}], body, truncated, char_count}`。

## SESSDATA Cookie

部分接口（高清晰度流、搜索、部分字幕）需要登录态。在浏览器登录 Bilibili 后，从 cookie 中复制 `SESSDATA` 的值，通过以下任一方式提供：

```bash
# 命令行参数
bili-cli --cookie <SESSDATA> search "关键词"

# 环境变量
export BILI_SESSDATA=<SESSDATA>
bili-cli search "关键词"
```

## 清晰度 `qn` 对照

| qn  | 清晰度          | qn  | 清晰度          |
| --- | --------------- | --- | --------------- |
| 127 | 8K 超高清       | 74  | 720P 60帧       |
| 126 | 杜比视界        | 64  | 720P 高清       |
| 125 | HDR 真彩        | 32  | 480P 流畅       |
| 120 | 4K 超清         | 16  | 360P 流畅       |
| 116 | 1080P 60帧      | 6   | 240P            |
| 112 | 1080P 高码率    | 0   | 自动选最高可用  |
| 100 | 1080P 高清      |     |                 |

## 项目结构

```
src/
  main.rs        入口与命令分发
  cli.rs         clap 命令行参数定义
  bvid.rs        BV/AV 号与 URL 解析、互转
  api.rs         Bilibili HTTP 客户端封装
  models.rs      API 响应数据结构
  commands/
    mod.rs       通用工具与子命令导出
    info.rs      解析视频信息
    search.rs    搜索视频
    links.rs     提取下载链接
    download.rs  下载视频 + ffmpeg 合并
    subtitle.rs  智能提取字幕
    transcript.rs LLM 友好逐字稿 + 无字幕降级指引
```

## License

MIT
