//! P1-6 L1 算子 B: Social Link Cleaner (Master Spec §3.3 Filter B)
//!
//! 仅清洗明确的社媒追踪参数, 绝不改写链接本体:
//!   - fbclid (Facebook) / gclid (Google) / ttclid (TikTok)
//!   - igshid (Instagram) / igsh (Instagram) / si (YouTube 分享) / spm (阿里系)
//!   - utm_* 全家 (utm_source / utm_medium / utm_campaign / utm_term / utm_content / utm_id)
//!
//! 铁律: 只删参数, 不删查询串本身; 不改短链、不展开、不猜测。
//! 正则预编译 (OnceLock), 单次 <5ms @64KB。

use regex::Regex;
use std::sync::OnceLock;

/// 追踪参数 key 判定 (大小写不敏感)
fn is_tracking_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    matches!(
        k.as_str(),
        "fbclid" | "gclid" | "ttclid" | "igshid" | "igsh" | "si" | "spm"
    ) || k.starts_with("utm_")
}

/// URL 正则: 协议 + 非空白主机路径 (query 与 fragment 由调用方拆分)
fn re_url() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"https?://[^\s<>"'\)\]]+"#).unwrap())
}

/// 清洗结果 (透明度: 记录删除条数供 UI 展示)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanResult {
    pub text: String,
    pub removed_count: usize,
}

/// 清洗单个 URL 的 query, 返回 (新URL, 删除条数)
fn clean_url(url: &str) -> (String, usize) {
    // 拆分 fragment (#...)
    let (before_frag, frag) = match url.find('#') {
        Some(i) => (&url[..i], Some(&url[i..])),
        None => (url, None),
    };
    // 拆分 query (?...)
    let (base, query) = match before_frag.find('?') {
        Some(i) => (&before_frag[..i], Some(&before_frag[i + 1..])),
        None => (before_frag, None),
    };
    let Some(query) = query else {
        return (url.to_string(), 0);
    };
    let mut kept = Vec::new();
    let mut removed = 0usize;
    for kv in query.split('&') {
        if kv.is_empty() {
            continue;
        }
        let key = kv.split('=').next().unwrap_or("");
        if is_tracking_key(key) {
            removed += 1;
        } else {
            kept.push(kv);
        }
    }
    let mut out = base.to_string();
    if !kept.is_empty() {
        out.push('?');
        out.push_str(&kept.join("&"));
    }
    if let Some(f) = frag {
        out.push_str(f);
    }
    (out, removed)
}

/// 清洗文本中的社媒追踪参数 (query 参数级重组, 保证 ?/& 边界正确)
pub fn clean_social_text(input: &str) -> CleanResult {
    let mut total_removed = 0usize;
    let out = re_url()
        .replace_all(input, |caps: &regex::Captures| {
            let (cleaned, removed) = clean_url(&caps[0]);
            total_removed += removed;
            cleaned
        })
        .into_owned();
    CleanResult {
        text: out,
        removed_count: total_removed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_fbclid() {
        let r = clean_social_text("https://x.dev/a?fbclid=IwAR2xyz&keep=1");
        assert_eq!(r.text, "https://x.dev/a?keep=1");
        assert!(r.removed_count >= 1);
    }

    #[test]
    fn removes_utm_family() {
        let r = clean_social_text("https://x.dev/a?utm_source=tw&utm_medium=social&keep=1");
        assert_eq!(r.text, "https://x.dev/a?keep=1");
    }

    #[test]
    fn removes_youtube_si_and_instagram_igsh() {
        let r = clean_social_text("https://youtu.be/v?si=abc");
        assert_eq!(r.text, "https://youtu.be/v");
        let r2 = clean_social_text("https://instagram.com/p?igsh=xyz");
        assert_eq!(r2.text, "https://instagram.com/p");
    }

    #[test]
    fn keeps_legit_params() {
        let r = clean_social_text("https://x.dev/a?id=42&q=hello");
        assert_eq!(r.text, "https://x.dev/a?id=42&q=hello");
        assert_eq!(r.removed_count, 0);
    }

    #[test]
    fn keeps_non_url_text_untouched() {
        let r = clean_social_text("普通文本没有链接");
        assert_eq!(r.text, "普通文本没有链接");
        assert_eq!(r.removed_count, 0);
    }

    #[test]
    fn multiple_urls_all_cleaned() {
        let r = clean_social_text(
            "看 https://a.dev?utm_source=x&keep=1 和 https://b.dev?fbclid=y",
        );
        assert!(r.text.contains("https://a.dev?keep=1"));
        assert!(r.text.contains("https://b.dev"));
        assert!(!r.text.contains("utm_source"));
        assert!(!r.text.contains("fbclid"));
    }

    #[test]
    fn trailing_param_leaves_clean_url() {
        let r = clean_social_text("https://x.dev/a?utm_campaign=z");
        assert_eq!(r.text, "https://x.dev/a");
    }

    #[test]
    fn case_insensitive_utm() {
        let r = clean_social_text("https://x.dev/a?UTM_SOURCE=x&keep=1");
        assert_eq!(r.text, "https://x.dev/a?keep=1");
    }
}
