//! 测试门禁 1/3: l1_snapshots — TDD §8 / Dev Plan P1-14
//!
//! L1 算子黄金快照门禁: 锁定 5 个 L1 算子的确定性输出形态,
//! 防止重构引入行为漂移 (Master Spec §3.3 语义契约)。
//!
//! 断言策略: 输出形态契约 (非字节级 golden, 便于中文/RTF 细节演进),
//! 核心不变量逐条锁定: D1 杀手滤镜三通道差异性 / IM 净化 / 追踪剥离 / DLP 静默。

use pasteflow_lib::filters::{
    clean_social_text, markdown_to_html, markdown_to_im_text, markdown_to_rtf,
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
