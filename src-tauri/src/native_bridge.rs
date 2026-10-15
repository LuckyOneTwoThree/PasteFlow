//! P1-11 Native Bridge — objc2/AppKit 装配层
//!
//! 将 menu_bar::MenuBarController / toast::NativeToastController 的纯逻辑状态机
//! 装配到真实 macOS 原生控件 (零 Webview):
//!   - NSStatusItem + NSMenu (菜单栏控制中心)
//!   - 非激活 NSPanel + NSTextField (原生 Toast)
//!
//! 铁律 (§8.3 R4): 全部 AppKit 调用严格在主线程; 后台线程仅通过状态机读取。
//!
//! 实现注记 (P1-11 范围): 本模块提供原生控件的创建与生命周期管理骨架,
//! 消息循环调度由 tauri 层 (P1-14) 统一接管。这里聚焦控件装配 + 状态机联动。

use crate::menu_bar::{MenuBarController, MenuItemDef};
use crate::toast::{ToastLevel, ToastRequest};
use parking_lot::Mutex;
use std::sync::Arc;

/// 原生控件持有占位 (真机 objc2 NSStatusItem/NSMenu 强引用在 tauri setup 钩子持有)
/// 注: P1-11 交付纯逻辑状态机 + 装配骨架; P1-14 在 tauri 层用 objc2 真实创建控件
/// 并把 Retained 句柄注入此结构 (字段为 opaque, 保持编译期与 objc2 解耦)
type NativeHandle = Option<Box<dyn std::any::Any + Send>>;

/// 原生菜单栏句柄 (持有 NSStatusItem 强引用, 防释放)
pub struct NativeMenuBar {
    /// opaque 持有位 (P1-14 tauri setup 注入 objc2 Retained; 字段只保活, 不读取)
    #[allow(dead_code)]
    status_item: NativeHandle,
    #[allow(dead_code)]
    menu: NativeHandle,
    controller: Arc<MenuBarController>,
}

impl NativeMenuBar {
    /// 创建菜单栏控制中心 (主线程调用)
    pub fn new(controller: Arc<MenuBarController>) -> Self {
        Self {
            status_item: None,
            menu: None,
            controller,
        }
    }

    /// 装配 NSStatusItem + NSMenu (主线程)
    /// 返回装配是否成功 (真机需 AppKit 主线程上下文; 测试环境返回 false)
    pub fn assemble(&mut self) -> bool {
        #[cfg(target_os = "macos")]
        {
            // 真机装配: NSStatusBar.systemStatusBar().statusItemWithLength(...)
            // 此处为骨架: 实际 objc2 调用在 tauri setup 钩子内执行 (P1-14)
            let defs = self.controller.menu_definition();
            tracing::info!(
                target: "pasteflow_lib::native_bridge",
                item_count = defs.len(),
                "MenuBar definition assembled (native binding in tauri setup)"
            );
            true
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    /// 刷新菜单状态 (启用开关 / 滤镜开关变化后)
    pub fn refresh(&self) -> Vec<MenuItemDef> {
        self.controller.menu_definition()
    }

    pub fn controller(&self) -> Arc<MenuBarController> {
        self.controller.clone()
    }
}

/// 原生 Toast 面板
pub struct NativeToastPanel {
    /// opaque 持有位 (P1-14 tauri setup 注入 objc2 Retained; 字段只保活, 不读取)
    #[allow(dead_code)]
    panel: NativeHandle,
    #[allow(dead_code)]
    label: NativeHandle,
    request: Mutex<Option<ToastRequest>>,
}

impl Default for NativeToastPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeToastPanel {
    pub fn new() -> Self {
        Self {
            panel: None,
            label: None,
            request: Mutex::new(None),
        }
    }

    /// 装配非激活 NSPanel (主线程): borderless + nonactivatingPanel, 不抢焦点
    pub fn assemble(&mut self) -> bool {
        #[cfg(target_os = "macos")]
        {
            // 真机装配: NSPanel(contentRect:styleMask:[.borderless,.nonactivatingPanel]...)
            // 实际 objc2 调用在 tauri setup 钩子内执行 (P1-14)
            tracing::info!(
                target: "pasteflow_lib::native_bridge",
                "Toast panel definition assembled (non-activating, native binding in tauri setup)"
            );
            true
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    /// 显示 Toast (主线程); SilentDlp 级别不含明文 (Master Spec §3.2)
    pub fn show(&mut self, req: ToastRequest) {
        // 静默三不: 文本已不含密钥明文 (toast.rs 测试保证)
        let shown = match req.level {
            ToastLevel::SilentDlp => "🔒 敏感信息已保护".to_string(),
            _ => req.text.clone(),
        };
        tracing::info!(
            target: "pasteflow_lib::native_bridge::toast",
            toast_text = %shown,
            "toast shown"
        );
        *self.request.lock() = Some(req);
    }

    /// 隐藏 Toast (主线程)
    pub fn hide(&mut self) {
        *self.request.lock() = None;
    }

    /// 当前 Toast 文本 (测试断言 / 渲染)
    pub fn current_display_text(&self) -> Option<String> {
        self.request.lock().as_ref().map(|r| match r.level {
            ToastLevel::SilentDlp => "🔒 敏感信息已保护".to_string(),
            _ => r.text.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_bar_assembles_and_refreshes() {
        let ctrl = Arc::new(MenuBarController::new());
        let mut mb = NativeMenuBar::new(ctrl);
        // 非 macOS 测试环境返回 false, macOS 测试返回 true (骨架)
        let _ = mb.assemble();
        let defs = mb.refresh();
        assert_eq!(defs.len(), 10);
    }

    #[test]
    fn toast_panel_shows_and_hides() {
        let mut tp = NativeToastPanel::new();
        tp.show(ToastRequest {
            text: "已复制".into(),
            level: ToastLevel::Info,
        });
        assert_eq!(tp.current_display_text().unwrap(), "已复制");
        tp.hide();
        assert!(tp.current_display_text().is_none());
    }

    #[test]
    fn toast_panel_silent_dlp_no_plaintext() {
        let mut tp = NativeToastPanel::new();
        tp.show(ToastRequest {
            text: "sk-abc123secretkey".into(),
            level: ToastLevel::SilentDlp,
        });
        let txt = tp.current_display_text().unwrap();
        assert!(!txt.contains("sk-"));
        assert!(txt.contains("敏感信息已保护"));
    }

    #[test]
    fn toast_panel_info_keeps_text() {
        let mut tp = NativeToastPanel::new();
        tp.show(ToastRequest {
            text: "已复制到剪贴板".into(),
            level: ToastLevel::Info,
        });
        assert_eq!(tp.current_display_text().unwrap(), "已复制到剪贴板");
    }
}
