# bili-cli

一个用 Rust 编写的 Bilibili 命令行工具：解析视频信息、搜索视频、提取下载链接、下载视频，并智能提取字幕。

## 功能

- `info` —— 解析视频元数据（标题、UP 主、播放/点赞/收藏统计、封面、分 P 列表等）
- `search` —— 按关键词搜索视频，表格化展示结果
- `links` —— 提取可播放的 DASH 流地址（视频/音频，按清晰度 `qn` 和编码分类）
- `download` —— 下载视频与音频流，并自动调用 `ffmpeg` 合并为 mp4（支持仅音频、跳过合并）
- `subtitle` —— 智能提取字幕：自动优先选择 `zh-Hans` > `zh-Hant` > `zh`（人工）> `AI` > `en`，输出 `srt` / `vtt` / `json` / `txt`

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
```

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
```

## License

MIT
