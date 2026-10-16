//! 测试门禁 1/3: l1_snapshots — TDD §8 / Dev Plan P1-14
//!
//! L1 算子黄金快照门禁: 锁定 5 个 L1 算子的确定性输出形态,
//! 防止重构引入行为漂移 (Master Spec §3.3 语义契约)。
//!
//! 断言策略: 输出形态契约 (非字节级 golden, 便于中文/RTF 细节演进),
//! 核心不变量逐条锁定: D1 杀手滤镜三通道差异性 / IM 净化 / 追踪剥离 / DLP 静默。

use pasteflow_lib::filters::{
    clean_social_text, clean_web_text, detect_dev_kind, markdown_to_html, markdown_to_im_text,
    markdown_to_rtf, reflow_pdf_text, DevPayloadKind,
};
use pasteflow_lib::privacy_guard::{DlpAction, DlpDetector};
use pasteflow_lib::pipe_executor::{classify_target, RouteClass, PipeExecutor};
use std::sync::Arc;

const SAMPLE_MD: &str = r#"# 标题
**加粗** 与 *斜体* 和 `code`

- 项目甲
- 项目乙

看 [文档](https://x.dev/a?utm_source=tw&keep=1) 谢谢"#;

// —— D1 杀手滤镜: 同一份 Markdown 三通道差异性 ——

#[test]
fn snapshot_d1_three_channels_diverge() {
    let html = markdown_to_html(SAMPLE_MD);
    let rtf = markdown_to_rtf(SAMPLE_MD);
    let im = markdown_to_im_text(SAMPLE_MD);
    // 三通道两两不同 (D1 核心断言)
    assert_ne!(html, rtf);
    assert_ne!(html, im);
    assert_ne!(rtf, im);
}

#[test]
fn snapshot_d1_html_keeps_rich_structure() {
    let html = markdown_to_html(SAMPLE_MD);
    assert!(html.contains("<h1>"));
    assert!(html.contains("<strong>"));
    assert!(html.contains("<em>") || html.contains("<i>"));
    assert!(html.contains("<ul>") || html.contains("<li>"));
    assert!(html.contains("https://x.dev/a"));
}

#[test]
fn snapshot_d1_rtf_starts_with_header() {
    let rtf = markdown_to_rtf(SAMPLE_MD);
    assert!(rtf.starts_with(r"{\rtf1"));
    assert!(rtf.ends_with('}'));
    assert!(rtf.contains(r"\fs32\b")); // h1 粗体
    assert!(rtf.contains(r"{\b ")); // bold 组
}

#[test]
fn snapshot_d1_im_strips_markup() {
    let im = markdown_to_im_text(SAMPLE_MD);
    assert!(im.contains("标题"));
    assert!(im.contains("【加粗】")); // ** -> 【】
    assert!(!im.contains("**"));
    assert!(!im.contains("```"));
    assert!(im.contains("文档 (https://x.dev/a?utm_source=tw&keep=1)")); // 链接展开
    assert!(im.contains(" • 项目甲")); // 列表归一
}

// —— Filter B: 社媒链接净化 ——

#[test]
fn snapshot_social_clean_strips_tracking() {
    let r = clean_social_text("https://x.dev/a?utm_source=tw&fbclid=zz&keep=1");
    assert_eq!(r.text, "https://x.dev/a?keep=1");
    assert!(r.removed_count >= 2);
}

#[test]
fn snapshot_social_clean_keeps_legit() {
    let r = clean_social_text("https://x.dev/a?id=42&q=hi");
    assert_eq!(r.text, "https://x.dev/a?id=42&q=hi");
    assert_eq!(r.removed_count, 0);
}

// —— DLP 静默三不 ——

#[test]
fn snapshot_dlp_silent_guard_flags_key() {
    let d = DlpDetector::new();
    assert_eq!(
        d.detect("sk-abc123def456ghi789jkl012mno"),
        DlpAction::SilentGuard
    );
    assert_eq!(d.detect("ghp_abcdefghijklmnopqrstuvwxyz0123456789AB"), DlpAction::SilentGuard);
    assert_eq!(d.detect("普通中文内容"), DlpAction::Normal);
}

// —— 路由决策树 (TDD §3.4) ——

#[test]
fn snapshot_routing_decision_tree() {
    assert_eq!(classify_target("com.tencent.xinWeChat"), RouteClass::ImCleanedText);
    assert_eq!(classify_target("com.tencent.WeWorkMac"), RouteClass::ImCleanedText);
    assert_eq!(classify_target("com.alibaba.DingTalkMac"), RouteClass::ImCleanedText);
    assert_eq!(classify_target("com.electron.lark"), RouteClass::ImCleanedText);
    assert_eq!(classify_target("com.tinyspeck.slackmacgap"), RouteClass::SuppressRich);
    assert_eq!(classify_target("com.microsoft.VSCode"), RouteClass::SuppressRich);
    assert_eq!(classify_target("com.apple.Terminal"), RouteClass::SuppressRich);
    assert_eq!(classify_target("com.microsoft.Word"), RouteClass::RichOffice);
    assert_eq!(classify_target("com.apple.TextEdit"), RouteClass::RichOffice);
}

#[test]
fn snapshot_rich_office_full_takeover() {
    let ex = PipeExecutor::new(Arc::new(DlpDetector::new()));
    let plan = ex.route(SAMPLE_MD, "com.microsoft.Word", 0);
    assert_eq!(plan.route, RouteClass::RichOffice);
    let html = plan.html.unwrap();
    let rtf = plan.rtf.unwrap();
    let plain = plan.plain.unwrap();
    assert!(html.contains("<h1>"));
    assert!(rtf.starts_with(r"{\rtf1"));
    assert_eq!(plain, SAMPLE_MD); // Office 通道保留原始 Markdown 源
}

#[test]
fn snapshot_suppress_dual_channel_nil() {
    // 铁律 R2: html 与 rtf 必须同步为 nil (防 Slack 回退读 RTF 劫持)
    let ex = PipeExecutor::new(Arc::new(DlpDetector::new()));
    let plan = ex.route(SAMPLE_MD, "com.tinyspeck.slackmacgap", 0);
    assert_eq!(plan.route, RouteClass::SuppressRich);
    assert!(plan.html.is_none());
    assert!(plan.rtf.is_none());
    assert_eq!(plan.plain.unwrap(), SAMPLE_MD); // 源码原样
}

#[test]
fn snapshot_im_cleaned_only_plain() {
    let ex = PipeExecutor::new(Arc::new(DlpDetector::new()));
    let plan = ex.route(SAMPLE_MD, "com.tencent.xinWeChat", 0);
    assert_eq!(plan.route, RouteClass::ImCleanedText);
    assert!(plan.html.is_none());
    assert!(plan.rtf.is_none());
    assert!(plan.plain.unwrap().contains("【加粗】"));
}

// —— P3-3: 【网页牛皮癣洗白】(Master Spec §10.2 #4) ——

#[test]
fn snapshot_web_cleaner_strips_boilerplate() {
    let src = r#"<p style="font-size:72px" class="hero">正文标题</p>
<span id="track">关键论点[1]</span>
© 2026 某某公司 版权所有
阅读原文
关注我们"#;
    let out = clean_web_text(src);
    // 样式/属性/冗余 span 剥离
    assert!(!out.contains("font-size"));
    assert!(!out.contains("class="));
    assert!(!out.contains("<span"));
    // 脚注编号剥离
    assert!(!out.contains("[1]"));
    assert!(out.contains("关键论点"));
    // 版权尾巴与推广语剥离
    assert!(!out.contains("版权所有"));
    assert!(!out.contains("阅读原文"));
    assert!(!out.contains("关注我们"));
    // 正文保留
    assert!(out.contains("正文标题"));
}

#[test]
fn snapshot_web_cleaner_collapses_blank_lines() {
    let out = clean_web_text("段落一。\n\n\n\n\n段落二。");
    assert!(!out.contains("\n\n\n"));
    assert!(out.contains("段落一。"));
    assert!(out.contains("段落二。"));
}

// —— P3-3: 【PDF 断行缝合】(Master Spec §10.2 #5) ——

#[test]
fn snapshot_pdf_reflow_joins_soft_wraps() {
    // 软回车缝合: 英文词间补空格, 跨行连字符合并
    let out = reflow_pdf_text("This is a sen-\ntence wrapped\nby PDF extraction.");
    assert!(!out.contains("\n"));
    assert!(out.contains("sentence wrapped by PDF extraction."));
}

#[test]
fn snapshot_pdf_reflow_keeps_hard_paragraphs() {
    // 句末标点结尾保留硬段落 (空行分隔)
    let out = reflow_pdf_text("第一段结束。\n第二段开始。");
    assert!(out.contains("第一段结束。\n\n第二段开始。"));
}

#[test]
fn snapshot_pdf_reflow_cjk_boundary_no_space() {
    // CJK 边界缝合不插入空格 (Master Spec §10.2 #5)
    let out = reflow_pdf_text("这是被 PDF\n截断的中文句子");
    assert_eq!(out, "这是被 PDF截断的中文句子");
}

// —— P3-3: 【开发者瑞士军刀】(Master Spec §10.2 #6, PASSTHROUGH + MANUAL_ONLY) ——

#[test]
fn snapshot_dev_swiss_detects_kinds() {
    assert_eq!(detect_dev_kind("{\"a\":1}"), DevPayloadKind::Json);
    assert_eq!(
        detect_dev_kind("curl https://x.dev/api -H 'a: b'"),
        DevPayloadKind::Curl
    );
    assert_eq!(detect_dev_kind("1729800000"), DevPayloadKind::UnixTimestamp);
    assert_eq!(
        detect_dev_kind("eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.sig"),
        DevPayloadKind::Jwt
    );
    assert_eq!(detect_dev_kind("aGVsbG8gd29ybGQ="), DevPayloadKind::Base64);
    assert_eq!(detect_dev_kind("普通句子 hello"), DevPayloadKind::Unknown);
}

#[test]
fn snapshot_dev_swiss_json_format_is_deterministic() {
    // 同输入两次输出一致 (确定性算子, TDD §7.4)
    let a = pasteflow_lib::filters::dev_swiss::format_json(r#"{"b":2,"a":1}"#).unwrap();
    let b = pasteflow_lib::filters::dev_swiss::format_json(r#"{"b":2,"a":1}"#).unwrap();
    assert_eq!(a, b);
    assert!(a.contains("\"a\": 1"));
}

#[test]
fn snapshot_dev_swiss_curl_to_fetch_keeps_url() {
    let out = pasteflow_lib::filters::dev_swiss::curl_to_fetch(
        "curl https://x.dev/api?keep=1 -X POST --data '{\"k\":1}'",
    )
    .unwrap();
    assert!(out.contains("https://x.dev/api?keep=1"));
    assert!(out.contains("fetch("));
    assert!(out.contains("POST"));
}

#[test]
fn snapshot_dev_swiss_timestamp_roundtrip() {
    // 秒级 Unix 时间戳 → ISO 8601 (UTC)
    let r = pasteflow_lib::filters::dev_swiss::decode_timestamp("1729800000").unwrap();
    assert_eq!(r, "2024-10-24T20:00:00Z");
}
