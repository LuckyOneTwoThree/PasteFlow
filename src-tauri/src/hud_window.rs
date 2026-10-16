//! P2-1 收尾: Micro-HUD 窗口装配 (IPC Spec v1.0 §1 `micro-hud`)
//!
//! 依契约：主进程启动后延迟 `2s` 在后台预热创建 `WebviewWindow`，保持
//! `visible: false` **常驻不销毁**，确保 `⌘C⌘C` `0ms` 唤醒。
//!
//! 本模块负责：
//!   - `preheat`：延迟创建隐藏常驻窗口（WKWebView 容器）
//!   - `broadcast_summon`：把 `HudSummonBus` 载荷 emit 给 `micro-hud`（IPC Spec §4）
//!   - `HudVisibilityTracker`：跟踪可见状态（供 `cmd_hud_visible` 反馈）
//!
//! 按键不泄漏由页面 `keydown` 拦截（capture + preventDefault）+ 面板
//! `NonActivatingHUDPanel`（hud_panel）共同保证。

use crate::hud::HudSummonPayload;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

/// HUD 窗口标识 (IPC Spec §1)
pub const HUD_WINDOW_LABEL: &str = "micro-hud";
/// HUD 页面路由 (`/#/hud` 由静态页面承载)
const HUD_URL: &str = "hud.html";
/// 预热延迟 (IPC Spec §1: 启动后延迟 2s 预热)
pub const PREHEAT_DELAY_MS: u64 = 2_000;

/// HUD 可见状态跟踪（跨线程原子）
#[derive(Debug, Default)]
pub struct HudVisibilityTracker {
    visible: AtomicBool,
}

impl HudVisibilityTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_visible(&self, visible: bool) {
        self.visible.store(visible, Ordering::Relaxed);
    }

    pub fn is_visible(&self) -> bool {
        self.visible.load(Ordering::Relaxed)
    }
}

/// 延迟 2s 预热创建常驻隐藏 HUD 窗口 (IPC Spec §1)
///
/// 失败时仅记日志（HUD 为增强件，不影响自动挡核心链路）。
pub fn preheat(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(PREHEAT_DELAY_MS));
        let _ = app.clone().run_on_main_thread(move || {
            if app.get_webview_window(HUD_WINDOW_LABEL).is_some() {
                return; // 已创建（重复预热短路）
            }
            match WebviewWindowBuilder::new(
                &app,
                HUD_WINDOW_LABEL,
                WebviewUrl::App(HUD_URL.into()),
            )
            .title("PasteFlow Micro-HUD")
            .inner_size(420.0, 240.0)
            .resizable(false)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .visible(false) // 常驻隐藏 (IPC Spec §1)
            .focused(false)
            .build()
            {
                Ok(_) => tracing::info!("micro-hud preheated (hidden, resident)"),
                Err(e) => tracing::warn!(error = %e, "micro-hud preheat failed"),
            }
        });
    });
}

/// 广播 HUD 唤醒载荷 (IPC Spec §4 `pasteflow://hud-summoned`)
///
/// 同时把面板置前（`show()` + `set_focus()`），实现 `⌘C⌘C` 0ms 唤醒。
pub fn broadcast_summon(app: &tauri::AppHandle, payload: &HudSummonPayload) {
    if let Some(win) = app.get_webview_window(HUD_WINDOW_LABEL) {
        // 面板置前但不抢焦点到自身之外（NonActivatingHUDPanel 行为由 hud_panel 保证）
        if let Err(e) = win.show() {
            tracing::warn!(error = %e, "micro-hud show failed");
        }
    }
    if let Err(e) = app.emit_to(HUD_WINDOW_LABEL, "pasteflow://hud-summoned", payload) {
        tracing::warn!(error = %e, "hud-summoned emit failed");
    }
}

/// 隐藏 HUD（Esc / 动作完成后）
pub fn hide(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window(HUD_WINDOW_LABEL) {
        let _ = win.hide();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visibility_tracker_toggles() {
        let t = HudVisibilityTracker::new();
        assert!(!t.is_visible());
        t.set_visible(true);
        assert!(t.is_visible());
        t.set_visible(false);
        assert!(!t.is_visible());
    }

    #[test]
    fn window_label_matches_ipc_contract() {
        // IPC Spec §1: 窗口标识为 `micro-hud`
        assert_eq!(HUD_WINDOW_LABEL, "micro-hud");
    }

    #[test]
    fn preheat_delay_is_2s() {
        // IPC Spec §1: 启动后延迟 2s 预热
        assert_eq!(PREHEAT_DELAY_MS, 2_000);
    }
}
