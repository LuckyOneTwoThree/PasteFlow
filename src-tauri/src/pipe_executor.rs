//! P1-7 PipeExecutor — TDD §3.4 / Master Spec §3.2
//!
//! Provider 回调路由决策树 (实测 0.042µs, O(1) 查表):
//!
//!   TargetApp ∈ IM_TARGET_BUNDLES  -> im_cleaned_text (仅 plainText)
//!   TargetApp ∈ SUPPRESS_BUNDLES   -> html/rtf 同步供给 nil, plainText = 原始源码
//!   否则 (Office/富文本应用)        -> rich_office (html+rtf+plainText 全通道)
//!
//! 附 P0 实测补丁 (§1.4 / Master Spec A.13):
//!   - Re-own 补挂触发: 首读命中 Probe Guard (<150ms & frontmost==SourceApp) 后,
//!     等全通道供给完成或 300ms 超时, 补挂新一代 Provider
//!   - 补挂 <= 3 代, 连续嗅探降级 StaticUnmanaged (路线 B)
//!   - 双通道同步抑制 (铁律 R2): SUPPRESS 判定对 html+rtf 同时生效, 防 Slack 回退读 RTF 劫持

use crate::privacy_guard::{DlpAction, DlpDetector};
use std::collections::HashSet;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// IM 纯文本净化白名单 (Master Spec §3.2)
fn im_targets() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| {
        HashSet::from([
            "com.tencent.xinWeChat",       // 微信
            "com.tencent.WeWorkMac",       // 企业微信
            "com.alibaba.DingTalkMac",     // 钉钉
            "com.electron.lark",           // 飞书 Lark
            "com.kingsoft.wpsoffice.mac",  // WPS (聊天场景)
        ])
    })
}

/// 富文本注入抑制名单 (Master Spec §3.2: 只贴纯文本, 防富文本劫持)
fn suppress_targets() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| {
        HashSet::from([
            "com.tinyspeck.slackmacgap",               // Slack
            "com.hnc.Discord",                         // Discord
            "com.microsoft.VSCode",                    // VSCode
            "com.todesktop.230313mzl4w4u92",           // Cursor
            "com.apple.Terminal",                      // Terminal
            "com.googlecode.iterm2",                  // iTerm2
            "dev.zed.Zed",                             // Zed
            "com.sublimetext.4",                       // Sublime
        ])
    })
}

/// 供电给 clipboard_owner 的路由分类
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteClass {
    /// Office/富文本: html+rtf+plainText 全通道 (Word/Pages/TextEdit/Notion 浏览器)
    RichOffice,
    /// IM 场景: 仅 plainText, 走 im_text 净化
    ImCleanedText,
    /// 抑制场景: html/rtf 同步 nil, plainText = 原始源码 (Slack/VSCode/Terminal)
    SuppressRich,
    /// 静态降级 (路线 B): 连续嗅探场景, 不再补挂
    StaticUnmanaged,
}

/// Provider 回调决策 (路由 + 各通道 payload + 预算信号)
#[derive(Debug, Clone)]
pub struct ServePlan {
    pub route: RouteClass,
    pub html: Option<String>,
    pub rtf: Option<String>,
    pub plain: Option<String>,
    pub dlp: DlpAction,
}

/// 首读 Probe Guard 判定输入
#[derive(Debug, Clone, Copy)]
pub struct ProbeGuardContext {
    /// 首次供给距挂载毫秒数
    pub elapsed_ms: u64,
    /// 首读时前台 bundle
    pub frontmost_bundle: &'static str,
    /// 挂载时 (复制事件) 的前台 bundle
    pub source_bundle: &'static str,
}

/// Probe Guard 判定: <150ms & frontmost == SourceApp = 嗅探者抢读 (P0 exp1)
pub fn is_probe_guard(ctx: &ProbeGuardContext) -> bool {
    ctx.elapsed_ms < 150 && ctx.frontmost_bundle == ctx.source_bundle
}

/// Re-own 补挂代数上限 (P0 exp4 推论: <=3 代后降级静态供给)
pub const REOWN_MAX_GENERATIONS: usize = 3;
/// Re-own 补挂等待窗口: 全通道供给完成或 300ms 超时取先到 (P0 exp5)
pub const REOWN_SETTLE_WINDOW: Duration = Duration::from_millis(300);

/// Re-own 补挂状态机 (每代)
#[derive(Debug, Default)]
pub struct ReownState {
    pub generation: usize,
    pub served_channels: usize,
    pub declared_channels: usize,
    pub triggered: bool,
}

impl ReownState {
    /// 记录一次通道供给; 返回是否应触发补挂 (全通道供给完成)
    pub fn note_served(&mut self) -> bool {
        self.served_channels = (self.served_channels + 1).min(self.declared_channels);
        !self.triggered
            && self.generation < REOWN_MAX_GENERATIONS
            && self.declared_channels > 0
            && self.served_channels >= self.declared_channels
    }

    /// 超时兜底 (300ms 到): 无论通道是否全供给完成, 强制判定补挂
    pub fn note_timeout(&mut self) -> bool {
        !self.triggered && self.generation < REOWN_MAX_GENERATIONS
    }

    /// 标记已补挂
    pub fn mark_triggered(&mut self) {
        self.triggered = true;
        self.generation += 1;
    }

    /// 是否已达上限 (降级静态供给)
    pub fn exhausted(&self) -> bool {
        self.generation >= REOWN_MAX_GENERATIONS
    }
}

/// 路由分类 (O(1) 查表)
pub fn classify_target(bundle_id: &str) -> RouteClass {
    if im_targets().contains(bundle_id) {
        RouteClass::ImCleanedText
    } else if suppress_targets().contains(bundle_id) {
        RouteClass::SuppressRich
    } else {
        RouteClass::RichOffice
    }
}

/// PipeExecutor: 绑定 L1 管道, 输出 ServePlan
pub struct PipeExecutor {
    dlp: Arc<DlpDetector>,
}

impl PipeExecutor {
    pub fn new(dlp: Arc<DlpDetector>) -> Self {
        Self { dlp }
    }

    /// 主路由 (P0 exp1 变体判定已验证与 §3.2 决策树一致):
    /// 输入原始 source (Markdown 或普通文本), 输出三通道供给方案
    pub fn route(&self, source_text: &str, target_bundle: &str, generation: usize) -> ServePlan {
        let dlp_action = self.dlp.detect(source_text);
        if generation >= REOWN_MAX_GENERATIONS {
            // 降级: 静态纯转发
            return ServePlan {
                route: RouteClass::StaticUnmanaged,
                html: None,
                rtf: None,
                plain: Some(source_text.to_string()),
                dlp: dlp_action,
            };
        }
        match classify_target(target_bundle) {
            RouteClass::ImCleanedText => ServePlan {
                route: RouteClass::ImCleanedText,
                html: None,
                rtf: None,
                plain: Some(crate::filters::markdown_to_im_text(source_text)),
                dlp: dlp_action,
            },
            RouteClass::SuppressRich => ServePlan {
                route: RouteClass::SuppressRich,
                // 铁律 R2: html/rtf 同步抑制 (同步, 非异步)
                html: None,
                rtf: None,
                plain: Some(source_text.to_string()),
                dlp: dlp_action,
            },
            RouteClass::RichOffice => ServePlan {
                route: RouteClass::RichOffice,
                html: Some(crate::filters::markdown_to_html(source_text)),
                rtf: Some(crate::filters::markdown_to_rtf(source_text)),
                plain: Some(source_text.to_string()),
                dlp: dlp_action,
            },
            RouteClass::StaticUnmanaged => unreachable!("handled above"),
        }
    }

    /// 补挂新一代前的探测保护供给 (P0 exp1): 150ms 内抢读者只拿到原始数据
    pub fn probe_guard_passthrough(source_text: &str) -> ServePlan {
        ServePlan {
            route: RouteClass::StaticUnmanaged,
            html: Some("<p>raw-html</p>".into()),
            rtf: Some(r"{\rtf1\ansi raw}".into()),
            plain: Some(source_text.to_string()),
            dlp: DlpAction::Normal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn executor() -> PipeExecutor {
        PipeExecutor::new(Arc::new(DlpDetector::new()))
    }

    // —— 分类 ——
    #[test]
    fn wechat_is_im_target() {
        assert_eq!(classify_target("com.tencent.xinWeChat"), RouteClass::ImCleanedText);
    }

    #[test]
    fn slack_is_suppressed() {
        assert_eq!(classify_target("com.tinyspeck.slackmacgap"), RouteClass::SuppressRich);
    }

    #[test]
    fn cursor_is_suppressed() {
        assert_eq!(classify_target("com.todesktop.230313mzl4w4u92"), RouteClass::SuppressRich);
    }

    #[test]
    fn word_is_rich_office() {
        assert_eq!(classify_target("com.microsoft.Word"), RouteClass::RichOffice);
    }

    // —— 路由 ——
    #[test]
    fn im_route_only_plain_cleaned() {
        let plan = executor().route("# T\n**B**", "com.tencent.xinWeChat", 0);
        assert_eq!(plan.route, RouteClass::ImCleanedText);
        assert!(plan.html.is_none());
        assert!(plan.rtf.is_none());
        assert_eq!(plan.plain.unwrap(), "T\n【B】");
    }

    #[test]
    fn suppress_route_dual_channel_nil() {
        // 铁律 R2: html 与 rtf 同步 nil
        let plan = executor().route("# T", "com.tinyspeck.slackmacgap", 0);
        assert_eq!(plan.route, RouteClass::SuppressRich);
        assert!(plan.html.is_none());
        assert!(plan.rtf.is_none());
        assert_eq!(plan.plain.unwrap(), "# T");
    }

    #[test]
    fn rich_office_full_three_channels() {
        let plan = executor().route("# T", "com.microsoft.Word", 0);
        assert_eq!(plan.route, RouteClass::RichOffice);
        assert!(plan.html.unwrap().contains("<h1>"));
        assert!(plan.rtf.unwrap().starts_with(r"{\rtf1"));
        assert_eq!(plan.plain.unwrap(), "# T");
    }

    #[test]
    fn generation_cap_degrades_to_static() {
        let plan = executor().route("# T", "com.microsoft.Word", REOWN_MAX_GENERATIONS);
        assert_eq!(plan.route, RouteClass::StaticUnmanaged);
        assert!(plan.html.is_none());
    }

    // —— DLP 混入 ——
    #[test]
    fn dlp_silent_guard_propagates() {
        let plan = executor().route("key sk-abc123def456ghi789jkl012x", "com.tencent.xinWeChat", 0);
        assert_eq!(plan.dlp, DlpAction::SilentGuard);
    }

    // —— Probe Guard ——
    #[test]
    fn probe_guard_fires_on_fast_read_from_source() {
        let ctx = ProbeGuardContext {
            elapsed_ms: 60,
            frontmost_bundle: "com.pasteflow.app",
            source_bundle: "com.pasteflow.app",
        };
        assert!(is_probe_guard(&ctx));
    }

    #[test]
    fn probe_guard_not_fired_after_150ms() {
        let ctx = ProbeGuardContext {
            elapsed_ms: 339,
            frontmost_bundle: "com.pasteflow.app",
            source_bundle: "com.pasteflow.app",
        };
        assert!(!is_probe_guard(&ctx));
    }

    #[test]
    fn probe_guard_not_fired_on_real_paste_other_app() {
        let ctx = ProbeGuardContext {
            elapsed_ms: 60,
            frontmost_bundle: "com.tencent.xinWeChat",
            source_bundle: "com.pasteflow.app",
        };
        assert!(!is_probe_guard(&ctx));
    }

    // —— Re-own 状态机 ——
    #[test]
    fn reown_triggers_after_all_channels_served() {
        let mut st = ReownState { declared_channels: 3, ..Default::default() };
        assert!(!st.note_served());
        assert!(!st.note_served());
        assert!(st.note_served()); // 3/3 -> 触发
        st.mark_triggered();
        assert_eq!(st.generation, 1);
    }

    #[test]
    fn reown_timeout_fallback() {
        let mut st = ReownState { declared_channels: 3, ..Default::default() };
        st.note_served(); // 1/3
        assert!(st.note_timeout()); // 超时兜底触发
    }

    #[test]
    fn reown_cap_exhausts() {
        let mut st = ReownState::default();
        for _ in 0..REOWN_MAX_GENERATIONS {
            st.note_served();
            st.mark_triggered();
        }
        assert!(st.exhausted());
    }

    #[test]
    fn probe_guard_passthrough_raw() {
        let plan = PipeExecutor::probe_guard_passthrough("# T");
        assert_eq!(plan.plain.unwrap(), "# T");
    }
}
