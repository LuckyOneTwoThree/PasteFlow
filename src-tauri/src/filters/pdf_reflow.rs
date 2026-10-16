//! L1 滤镜【PDF 断行缝合】(Master Spec §10.2 #5)
//!
//! 自动识别中英文 PDF 复制产生的硬回车断行与跨行连字符，
//! 智能合并为通顺自然段。输出 `CLEANED` 通道策略。
//!
//! 规则（全部预编译, 零 NSDataDetector）：
//!   1. 跨行连字符断词合并（`exam-\nple` → `example`；中文连字符直删）
//!   2. 软回车缝合（行尾非句末标点 → 与下一行合并，中英文间不插空格）
//!   3. 硬段落保留（句末标点（。！？.!?）结尾 → 换行保留）
//!   4. 空行收敛

use regex::Regex;
use std::sync::OnceLock;

struct Rules {
    /// 跨行连字符: `word-\nword` (英文断词) 或 `字-\n字` (中文)
    hyphen_break: Regex,
    /// 句末标点结尾行（保留段落边界）
    sentence_end: Regex,
    /// 连续空行
    blank_lines: Regex,
}

fn rules() -> &'static Rules {
    static RULES: OnceLock<Rules> = OnceLock::new();
    RULES.get_or_init(|| Rules {
        hyphen_break: Regex::new(r"(\w)-\n(\w)").expect("static regex"),
        sentence_end: Regex::new(r#"[。！？.!?]["'”’)\]]*$"#).expect("static regex"),
        blank_lines: Regex::new(r"\n{3,}").expect("static regex"),
    })
}

/// PDF 断行缝合：硬回车断行 → 自然段
///
/// 中英文混合场景：中文行尾直接缝合（不插空格）；英文词间补空格。
pub fn reflow_pdf_text(source: &str) -> String {
    let r = rules();
    // Step 1: 跨行连字符断词合并
    let s = r.hyphen_break.replace_all(source, "$1$2").to_string();

    // Step 2/3: 按行处理, 软回车缝合 / 硬段落保留
    let mut out: Vec<String> = Vec::new();
    let mut buf = String::new();
    for line in s.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            // 空行 = 段落边界
            if !buf.is_empty() {
                out.push(std::mem::take(&mut buf));
            }
            continue;
        }
        if buf.is_empty() {
            buf.push_str(trimmed);
            continue;
        }
        // 句末标点结尾 → 硬段落，先冲出
        if r.sentence_end.is_match(&buf) {
            out.push(std::mem::take(&mut buf));
            buf.push_str(trimmed);
            continue;
        }
        // 软回车缝合：中英文边界不插空格，英文词间补空格
        let buf_end = buf.chars().last().unwrap_or(' ');
        let line_start = trimmed.chars().next().unwrap_or(' ');
        let cjk = |c: char| {
            matches!(c as u32,
                0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x3000..=0x303F | 0xFF00..=0xFFEF)
        };
        if cjk(buf_end) || cjk(line_start) {
            buf.push_str(trimmed);
        } else {
            buf.push(' ');
            buf.push_str(trimmed);
        }
    }
    if !buf.is_empty() {
        out.push(buf);
    }

    // Step 4: 空行收敛
    let joined = out.join("\n\n");
    r.blank_lines.replace_all(&joined, "\n\n").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reflows_soft_line_break_chinese() {
        let s = reflow_pdf_text("这是第一行断\n行缝合测试。");
        assert_eq!(s, "这是第一行断行缝合测试。");
    }

    #[test]
    fn reflows_soft_line_break_english_with_space() {
        let s = reflow_pdf_text("this is a sen-\ntence with hard\nbreaks");
        // 跨行连字符合并 + 英文词间补空格
        assert!(s.contains("sentence"));
        assert!(s.contains("hard breaks"));
    }

    #[test]
    fn preserves_hard_paragraphs() {
        let s = reflow_pdf_text("第一段结束。\n第二段开始。");
        assert_eq!(s, "第一段结束。\n\n第二段开始。");
    }

    #[test]
    fn merges_cross_line_hyphen_english() {
        let s = reflow_pdf_text("inter-\nnational");
        assert_eq!(s, "international");
    }

    #[test]
    fn handles_mixed_chinese_english() {
        let s = reflow_pdf_text("这是中文 English 混\n合文本测试。");
        assert!(s.contains("混合文本测试"));
    }

    #[test]
    fn blank_line_separates_paragraphs() {
        let s = reflow_pdf_text("段落一。\n\n段落二。");
        assert_eq!(s, "段落一。\n\n段落二。");
    }

    #[test]
    fn question_mark_preserves_paragraph() {
        let s = reflow_pdf_text("这是问题？\n这是回答。");
        assert_eq!(s, "这是问题？\n\n这是回答。");
    }

    #[test]
    fn collapses_multiple_blank_lines() {
        let s = reflow_pdf_text("段落。\n\n\n\n\n下一段。");
        assert_eq!(s, "段落。\n\n下一段。");
    }
}
