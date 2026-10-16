//! L1 滤镜【网页牛皮癣洗白】(Master Spec §10.2 #4)
//!
//! 剥离网页复制携带的刺眼背景色、内联字号、冗余 span 与版权尾巴，
//! 仅保留语义化段落、加粗与列表。输出 `CLEANED` 通道策略。
//!
//! TDD §7.4: 全部预编译正则, 零 NSDataDetector, <5ms @64KB。

use regex::Regex;
use std::sync::OnceLock;

struct Rules {
    /// style 属性整体剥离
    style_attr: Regex,
    /// class/id 属性剥离
    class_id_attr: Regex,
    /// 冗余 span 标签剥离（保留内容）
    span_tag: Regex,
    /// 版权尾巴（© 2024 xxx / 版权所有 / All rights reserved / 阅读原文 / 关注我们 …）
    copyright_tail: Regex,
    /// 脚注编号 [1] [2]（网页复制常带）
    footnote_mark: Regex,
    /// 连续空行收敛
    blank_lines: Regex,
}

fn rules() -> &'static Rules {
    static RULES: OnceLock<Rules> = OnceLock::new();
    RULES.get_or_init(|| Rules {
        style_attr: Regex::new(r#"\s+style="[^"]*""#).expect("static regex"),
        class_id_attr: Regex::new(r#"\s+(class|id)="[^"]*""#).expect("static regex"),
        span_tag: Regex::new(r"</?span[^>]*>").expect("static regex"),
        copyright_tail: Regex::new(
            r"(?im)^\s*(©|\(c\)|copyright)?\s*\d{4}[^\n]{0,30}(版权所有|版权|all rights reserved)[^\n]*$|^\s*(阅读原文|点击了解更多|关注我们|分享到|点赞|收藏|转发)[^\n]*$",
        )
        .expect("static regex"),
        footnote_mark: Regex::new(r"\[\d{1,3}\]").expect("static regex"),
        blank_lines: Regex::new(r"\n{3,}").expect("static regex"),
    })
}

/// 网页牛皮癣洗白: 清洗 HTML → 纯语义文本
///
/// 保留段落、加粗、列表；剥离样式/冗余 span/版权尾巴/脚注编号。
pub fn clean_web_text(source: &str) -> String {
    let r = rules();
    let mut s = source.to_string();
    // 剥离刺眼样式与冗余属性
    s = r.style_attr.replace_all(&s, "").to_string();
    s = r.class_id_attr.replace_all(&s, "").to_string();
    // 冗余 span 剥离（保留内容）
    s = r.span_tag.replace_all(&s, "").to_string();
    // 版权尾巴与推广语剥离
    s = r.copyright_tail.replace_all(&s, "").to_string();
    // 脚注编号剥离
    s = r.footnote_mark.replace_all(&s, "").to_string();
    // 连续空行收敛为单空行
    s = r.blank_lines.replace_all(&s, "\n\n").to_string();
    s.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_inline_style_and_class() {
        let s = clean_web_text(r#"<p style="background:#ff0000;font-size:72px" class="hero">标题</p>"#);
        assert_eq!(s, "<p>标题</p>");
    }

    #[test]
    fn strips_redundant_span_keeps_content() {
        let s = clean_web_text(r#"<p><span class="x">正文内容</span></p>"#);
        assert_eq!(s, "<p>正文内容</p>");
    }

    #[test]
    fn strips_copyright_tail() {
        let s = clean_web_text("文章正文。\n© 2026 某某公司 版权所有\n阅读原文\n");
        assert!(s.contains("文章正文"));
        assert!(!s.contains("版权所有"));
        assert!(!s.contains("阅读原文"));
    }

    #[test]
    fn strips_footnote_marks() {
        let s = clean_web_text("研究表明[1]这一点[23]成立。");
        assert_eq!(s, "研究表明这一点成立。");
    }

    #[test]
    fn keeps_semantic_paragraphs_bold_list() {
        let src = "<p>段落</p><b>加粗</b><ul><li>列表项</li></ul>";
        let s = clean_web_text(src);
        assert!(s.contains("<p>段落</p>"));
        assert!(s.contains("<b>加粗</b>"));
        assert!(s.contains("<li>列表项</li>"));
    }

    #[test]
    fn collapses_blank_lines() {
        let s = clean_web_text("第一段。\n\n\n\n第二段。");
        assert!(s.contains("第一段。\n\n第二段。"));
    }

    #[test]
    fn chinese_copyright_tail_removed() {
        let s = clean_web_text("正文内容\n关注我们，了解更多\n");
        assert!(s.contains("正文内容"));
        assert!(!s.contains("关注我们"));
    }
}
