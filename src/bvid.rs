use anyhow::{anyhow, bail, Result};
use regex::Regex;

/// Normalized identifier for a Bilibili video.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoId {
    Bvid(String),
    Aid(u64),
}

impl VideoId {
    pub fn as_query_pair(&self) -> (String, String) {
        match self {
            VideoId::Bvid(b) => ("bvid".to_string(), b.clone()),
            VideoId::Aid(a) => ("aid".to_string(), a.to_string()),
        }
    }

    pub fn label(&self) -> String {
        match self {
            VideoId::Bvid(b) => b.clone(),
            VideoId::Aid(a) => format!("av{}", a),
        }
    }
}

/// Parse a raw user input (BV1xx..., av123, 123, or a full b23/bilibili URL)
/// into a normalized `VideoId`.
pub fn parse_id(input: &str) -> Result<VideoId> {
    let s = input.trim();

    // Direct BV id: BV + 10 chars
    let bvid_re = Regex::new(r"^(BV[0-9A-Za-z]{10})$").unwrap();
    if let Some(c) = bvid_re.captures(s) {
        return Ok(VideoId::Bvid(c.get(1).unwrap().as_str().to_string()));
    }

    // AV id with optional prefix
    let av_re = Regex::new(r"^(?:av)?(\d+)$").unwrap();
    if let Some(c) = av_re.captures(s) {
        let n: u64 = c.get(1).unwrap().as_str().parse()?;
        return Ok(VideoId::Aid(n));
    }

    // Full URL: extract bvid or aid from the path/query.
    let url_bvid_re = Regex::new(r"(/BV[0-9A-Za-z]{10})").unwrap();
    let url_av_re = Regex::new(r"[?&]aid=(\d+)").unwrap();
    let url_av_path_re = Regex::new(r"/av(\d+)").unwrap();

    if let Some(c) = url_bvid_re.captures(s) {
        return Ok(VideoId::Bvid(c.get(1).unwrap().as_str()[1..].to_string()));
    }
    if let Some(c) = url_av_re.captures(s) {
        return Ok(VideoId::Aid(c.get(1).unwrap().as_str().parse()?));
    }
    if let Some(c) = url_av_path_re.captures(s) {
        return Ok(VideoId::Aid(c.get(1).unwrap().as_str().parse()?));
    }

    bail!("Could not parse video id from: {input}")
}

/// Convert BV id <-> AV id using Bilibili's base-58 algorithm (used as a
/// fallback when an endpoint only accepts one form).
#[allow(dead_code)]
pub fn bv_to_av(bvid: &str) -> Result<u64> {
    if !bvid.starts_with("BV") || bvid.len() != 12 {
        bail!("invalid bvid: {bvid}");
    }
    const TABLE: &[u8] = b"fZodR9XQDSUm21yCkr6zBqiveYah8bt4xsWpHnJE7jL5VG3guMTKNPAwcF";
    const TR: [u8; 6] = [9, 8, 1, 6, 2, 4];
    const XOR: u64 = 177451812;
    const ADD: u64 = 8728348608;

    let mut r: u64 = 0;
    for i in 0..6 {
        let ch = bvid.as_bytes()[TR[i] as usize];
        let idx = TABLE
            .iter()
            .position(|&t| t == ch)
            .ok_or_else(|| anyhow!("invalid bvid character: {}", ch as char))?;
        r += (idx as u64) * 58u64.pow(i as u32);
    }
    Ok((r - ADD) ^ XOR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bvid() {
        assert_eq!(
            parse_id("BV17x411w7KC").unwrap(),
            VideoId::Bvid("BV17x411w7KC".into())
        );
    }

    #[test]
    fn parses_av() {
        assert_eq!(parse_id("av170001").unwrap(), VideoId::Aid(170001));
        assert_eq!(parse_id("170001").unwrap(), VideoId::Aid(170001));
    }

    #[test]
    fn parses_url() {
        assert_eq!(
            parse_id("https://www.bilibili.com/video/BV17x411w7KC/").unwrap(),
            VideoId::Bvid("BV17x411w7KC".into())
        );
        assert_eq!(
            parse_id("https://b23.tv/av170001?aid=170001").unwrap(),
            VideoId::Aid(170001)
        );
    }

    #[test]
    fn bv_to_av_valid_and_invalid() {
        // valid 12-char BV string decodes to a positive av id
        let aid = bv_to_av("BV17x411w7KC").unwrap();
        assert!(aid > 0, "decoded aid should be positive, got {aid}");

        // wrong prefix rejected
        assert!(bv_to_av("XX17x411w7KC").is_err());
        // wrong length rejected
        assert!(bv_to_av("BV17x411w7K").is_err());
        assert!(bv_to_av("BV17x411w7KCC").is_err());
        // non-base58 character at an algorithm-read position (pos 9) rejected
        assert!(bv_to_av("BV17x411w!KC").is_err());
    }
}
