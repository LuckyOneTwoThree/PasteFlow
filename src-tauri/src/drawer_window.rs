//! P3-2 收尾: 磁吸式历史与贪吃蛇侧边抽屉窗口 (UI Spec v1.0 §4 / IPC Spec §1 `drawer`)
//!
//! 契约：首次按下 `⌥⇧V`（或菜单栏「剪贴板历史抽屉」）时按需创建 `360px` 无边框
//! 透明 `WebviewWindow`，磁吸停靠屏幕右侧边缘；关闭时仅 `hide()`（不销毁），
//! 下次唤醒 `0ms` 复用同一 Webview。
//!
//! 本模块只负责窗口生命周期；面板布局与交互属 `dist/drawer.html`。

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

/// 抽屉窗口标识 (IPC Spec §1)
pub const DRAWER_LABEL: &str = "drawer";
/// 抽屉页面
const DRAWER_URL: &str = "drawer.html";
/// 抽屉宽 (UI Spec §4.1)
pub const DRAWER_WIDTH: f64 = 360.0;
/// 屏幕右侧悬浮边距 (UI Spec §4.1: 上下各留 12px, 停靠右缘)
pub const DRAWER_MARGIN: f64 = 12.0;

/// 切换显示/隐藏（`⌥⇧V` / 菜单项）；首次调用时创建
pub fn toggle(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window(DRAWER_LABEL) {
        if win.is_visible().unwrap_or(false) {
            let _ = win.hide();
        } else {
            let _ = win.show();
        }
        return;
    }
    // 磁吸停靠屏幕右侧: x = 屏幕宽 - 抽屉宽 - 边距
    let (x, y, height) = dock_metrics(app);
    let _ = WebviewWindowBuilder::new(app, DRAWER_LABEL, WebviewUrl::App(DRAWER_URL.into()))
        .title("PasteFlow 抽屉")
        .inner_size(DRAWER_WIDTH, height)
        .position(x, y)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(true)
        .build();
}

/// 隐藏抽屉（Esc / 点击抽屉外区域 / 动作完成后）
pub fn hide(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window(DRAWER_LABEL) {
        let _ = win.hide();
    }
}

#[derive(serde::Deserialize)]
pub struct DrawerVisibleArgs {
    pub visible: bool,
}

/// 抽屉可见性命令（页面 Esc 关闭时调用, IPC Spec §3.2）
#[tauri::command]
pub fn cmd_drawer_visible(app: tauri::AppHandle, args: DrawerVisibleArgs) {
    if args.visible {
        if let Some(win) = app.get_webview_window(DRAWER_LABEL) {
            let _ = win.show();
        }
    } else {
        hide(&app);
    }
}

/// 停靠几何 (UI Spec §4.1: 高 = 屏幕可用高 - 24px, 上下各 12px)
fn dock_metrics(app: &tauri::AppHandle) -> (f64, f64, f64) {
    if let Some(m) = app.primary_monitor().ok().flatten() {
        let scale = m.scale_factor();
        let w = m.size().width as f64 / scale;
        let h = m.size().height as f64 / scale;
        let x = (w - DRAWER_WIDTH - DRAWER_MARGIN).max(0.0);
        return (x, DRAWER_MARGIN, (h - 2.0 * DRAWER_MARGIN).max(200.0));
    }
    // 回退: 主屏未知时用保守几何
    (1000.0, DRAWER_MARGIN, 700.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawer_label_matches_ipc_contract() {
        // IPC Spec §1: 窗口标识为 `drawer`
        assert_eq!(DRAWER_LABEL, "drawer");
    }

    #[test]
    fn drawer_width_is_360() {
        // UI Spec §4.1: 抽屉宽 360px
        assert_eq!(DRAWER_WIDTH, 360.0);
    }
}
