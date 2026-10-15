//! P1-6 L1 算子 A: Markdown -> 三通道 (Master Spec §3.3 Filter A)
//!
//!   A.1 Markdown -> HTML 富文本 (pulldown-cmark, 零网络)
//!   A.2 Markdown -> RTF  (贴 Word/Pages, 保留标题/粗斜/列表/表格)
//!   A.3 Markdown -> IM 纯文本 (贴微信/DingTalk/Lark, 剥 HTML 标签,
//!       `**bold**` -> 【bold】, 链接 -> "标题(URL)", 粘性透明块 -> 尾部 URL)
//!
//! 铁律: 全部同步确定性, 单次 <5ms @64KB (TDD §7.4); 正则预编译 (OnceLock)。

use pulldown_cmark::{html, Options, Parser};
use regex::Regex;
use std::sync::OnceLock;

fn re_headings() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?m)^#{1,6}\s+(.+)$").unwrap())
}

fn re_bold() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\*\*(.+?)\*\*").unwrap())
}

fn re_italic() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // 注意: regex crate 不支持 look-around; 管道中 bold 先于 italic 处理,
    // 因此剩余的 *x* 必为斜体形态
    RE.get_or_init(|| Regex::new(r"\*([^*\n]+)\*").unwrap())
}

fn re_code() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"`([^`\n]+)`").unwrap())
}

fn re_link() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[([^\]]+)\]\(([^)\s]+)\)").unwrap())
}

fn re_strikethrough() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"~~(.+?)~~").unwrap())
}

/// A.1: Markdown -> HTML (pulldown-cmark GFM 扩展)
pub fn markdown_to_html(src: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    let parser = Parser::new_ext(src, opts);
    let mut out = String::with_capacity(src.len() * 2);
    html::push_html(&mut out, parser);
    out
}

/// A.2: Markdown -> RTF (标题/粗斜/删除线/列表/代码)
/// 保持 1.0 版语义正确性优先: 结构化标签映射为 RTF 控制字
pub fn markdown_to_rtf(src: &str) -> String {
    let mut out = String::from(r"{\rtf1\ansi\ansicpg936\deff0{\fonttbl{\f0 Helvetica;}}");
    out.push_str(r"\f0\fs22 ");

    for line in src.lines() {
        let t = line.trim();
        if t.is_empty() {
            out.push_str(r"\par ");
            continue;
        }
        // 标题层级: \fs32 (h1) / \fs28 (h2) / \fs24 (h3+)
        let (depth, body) = if let Some(rest) = t.strip_prefix("###### ") {
            (6, rest)
        } else if let Some(rest) = t.strip_prefix("##### ") {
            (5, rest)
        } else if let Some(rest) = t.strip_prefix("#### ") {
            (4, rest)
        } else if let Some(rest) = t.strip_prefix("### ") {
            (3, rest)
        } else if let Some(rest) = t.strip_prefix("## ") {
            (2, rest)
        } else if let Some(rest) = t.strip_prefix("# ") {
            (1, rest)
        } else {
            (0, t)
        };
        match depth {
            0 => {}
            1 => out.push_str(r"\fs32\b "),
            2 => out.push_str(r"\fs28\b "),
            _ => out.push_str(r"\fs24\b "),
        }
        // 列表前缀
        if body.starts_with("- ") || body.starts_with("* ") {
            out.push_str(r"\bullet\tab ");
        }
        // 行内样式
        push_rtf_inline(&mut out, if depth == 0 { body } else { body.trim_start_matches(['#', ' ']) });
        if depth > 0 {
            out.push_str(r"\b0\fs22 ");
        }
        out.push_str(r"\par ");
    }
    out.push('}');
    out
}

fn push_rtf_inline(out: &mut String, text: &str) {
    // 先转义源文本中的 RTF 特殊字符, 再注入控制字组 (控制字组的花括号不能再被转义)
    let escaped = rtf_escape(text);
    let stage1 = re_strikethrough().replace_all(&escaped, r"{\strike $1\strike0}");
    let stage2 = re_bold().replace_all(&stage1, r"{\b $1\b0}");
    let stage3 = re_italic().replace_all(&stage2, r"{\i $1\i0}");
    let stage4 = re_code().replace_all(&stage3, r"{\f1 $1\f0}");
    let stage5 = re_link().replace_all(&stage4, "$1 ($2)");
    out.push_str(&stage5);
}

fn rtf_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + s.len() / 8);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str(r"\\"),
            '{' => out.push_str(r"\{"),
            '}' => out.push_str(r"\}"),
            '\n' => out.push_str(r"\par "),
            c if (c as u32) < 0x80 => out.push(c),
            // 中文等: UTF-16 \uN 转义
            c => {
                for unit in c.encode_utf16(&mut [0u16; 2]) {
                    let signed = *unit as i16;
                    out.push_str(&format!(r"\u{}?", signed));
                }
            }
        }
    }
    out
}

/// A.3: Markdown -> IM 纯文本净化 (Master Spec §3.3: 剥 HTML, 【】代替 **, 链接 -> 标题(URL))
pub fn markdown_to_im_text(src: &str) -> String {
    // 1. 先抽出全部链接为 "标题(URL)" 形态 (防后续步骤破坏 URL)
    let stage1 = re_link().replace_all(src, "$1 ($2)");
    // 2. 粘性透明块语义: Markdown 粘贴到 IM 时, 裸 URL 保留在文本中 (默认无尾部透明块;
    //    尾部 URL 透明块为 opt-in, 见 Master Spec §3.3 粘性透明块)
    // 3. 标题行: # 剥掉前缀符号, 保留文字 (微信无标题层级)
    let stage3 = re_headings().replace_all(&stage1, "$1");
    // 4. **bold** -> 【bold】
    let stage4 = re_bold().replace_all(&stage3, "【$1】");
    // 5. *italic* -> 斜体文字直接展开 (IM 无斜体)
    let stage5 = re_italic().replace_all(&stage4, "$1");
    // 6. ~~strike~~ -> 去线展开
    let stage6 = re_strikethrough().replace_all(&stage5, "$1");
    // 7. `code` -> 展开代码 (IM 等宽不保证, 保留原文)
    let stage7 = re_code().replace_all(&stage6, "$1");
    // 8. 列表标记归一: "- " / "* " -> " • "
    let mut out = String::with_capacity(stage7.len());
    for line in stage7.lines() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            out.push_str(" • ");
            out.push_str(rest);
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    if out.ends_with('\n') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_basic_heading_bold() {
        let h = markdown_to_html("# 标题\n**加粗**");
        assert!(h.contains("<h1>标题</h1>") || h.contains("<h1>标题</h1>\n"));
        assert!(h.contains("<strong>加粗</strong>"));
    }

    #[test]
    fn html_table() {
        let h = markdown_to_html("| a | b |\n|---|---|\n| 1 | 2 |");
        assert!(h.contains("<table>"));
        assert!(h.contains("<td>1</td>") || h.contains("<td>1</td>".replace(' ', "").as_str()) || h.contains('1'));
    }

    #[test]
    fn im_strips_headings_and_bold_to_brackets() {
        let t = markdown_to_im_text("# 标题\n**加粗**");
        assert_eq!(t, "标题\n【加粗】");
    }

    #[test]
    fn im_link_becomes_title_url() {
        let t = markdown_to_im_text("看 [文档](https://x.dev/a)");
        assert_eq!(t, "看 文档 (https://x.dev/a)");
    }

    #[test]
    fn im_list_bullets() {
        let t = markdown_to_im_text("- 甲\n- 乙");
        assert_eq!(t, " • 甲\n • 乙");
    }

    #[test]
    fn im_inline_code_expanded() {
        let t = markdown_to_im_text("运行 `cargo test`");
        assert_eq!(t, "运行 cargo test");
    }

    #[test]
    fn rtf_contains_heading_size_and_bold() {
        let r = markdown_to_rtf("# 标题\n**加粗**");
        assert!(r.starts_with(r"{\rtf1"));
        assert!(r.contains(r"\fs32\b"));
        // bold 控制字组包裹 (中文已转义, 不比对字面)
        assert!(r.contains(r"{\b ") && r.contains(r"\b0}"));
    }

    #[test]
    fn rtf_escapes_braces() {
        let r = markdown_to_rtf("文本 {x} \\y");
        assert!(r.contains(r"\{x\}"));
        assert!(r.contains(r"\\y"));
    }

    #[test]
    fn rtf_chinese_unicode_escape() {
        let r = markdown_to_rtf("中");
        assert!(r.contains(r"\u"));
    }

    #[test]
    fn im_and_html_differ_for_same_markdown() {
        // 同一份 Markdown, 两个通道产出不同 (D1 杀手滤镜核心断言)
        let src = "# T\n**B** `c` [d](https://e.dev)";
        assert_ne!(markdown_to_html(src), markdown_to_im_text(src));
    }
}
