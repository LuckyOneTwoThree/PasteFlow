//! P1-11 MenuBar 控制中心 — Dev Plan P1-11 / UI Spec §2.1 / IPC Spec §3.1 (MenuBar Menu Definition)
//!
//! 原生 NSStatusItem + NSMenu (零 Webview, 55MB 内存红线):
//!   [菜单栏图标 (拼贴板样式)]
//!   ├─ ✅ 启用 PasteFlow          (开关, NSMenuItem.state)
//!   ├─ ────────────────────
//!   ├─  Markdown → 富文本      (L1 杀手滤镜 A)
//!   ├─ 🧹 社媒链接净化            (L1 杀手滤镜 B)
//!   ├─ ────────────────────
//!   ├─ 📋 打开剪贴板历史          (历史窗口)
//!   ├─ 📊 诊断看板               (诊断窗口)
//!   ├─ ────────────────────
//!   └─ 🚪 退出 PasteFlow
//!
//! `menu_definition()` 是菜单结构的唯一真源 (IPC Spec §3.1 MenuBar Menu Definition),
//! 托盘装配与 IPC 层均以此模型为准, 避免两套菜单模型漂移。
//!
//! 状态机 (纯逻辑, UI 层装配调用):
//!   启用开关 -> materializer.run_lifecycle(Quit/Pause) 物化接管后停止轮询

use parking_lot::Mutex;
use std::sync::Arc;

/// 菜单项 ID (IPC Spec §3.1 MenuBar Menu Definition)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuItemId {
    /// 启用/暂停 PasteFlow 接管
    EnableToggle,
    /// 剪贴板历史 (P2, v0.1.5)
    History,
    /// L1 滤镜 A: Markdown -> 富文本
    FilterMarkdown,
    /// L1 滤镜 B: 社媒链接净化
    FilterSocialLink,
    /// 偏好设置 (P2)
    Preferences,
    /// 诊断看板 (P2)
    Diagnostics,
    /// 退出
    Quit,
}

/// 菜单项状态 (NSMenuItem.state: .on / .off)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuState {
    On,
    Off,
}

/// 菜单项定义 (结构化, UI 层由 IPC MenuBar Menu Definition 渲染)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItemDef {
    pub id: MenuItemId,
    pub label: String,
    pub enabled: bool,
    pub state: MenuState,
    /// 分隔符 (纯视觉, 不可点击)
    pub is_separator: bool,
}

impl MenuItemDef {
    pub fn item(id: MenuItemId, label: &str, enabled: bool, state: MenuState) -> Self {
        Self {
            id,
            label: label.to_string(),
            enabled,
            state,
            is_separator: false,
        }
    }

    pub fn separator() -> Self {
        Self {
            id: MenuItemId::Quit, // 占位
            label: String::new(),
            enabled: false,
            state: MenuState::Off,
            is_separator: true,
        }
    }
}

/// 控制中心事件 (菜单点击 -> 引擎动作)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    /// 启用/暂停切换
    ToggleEnabled,
    /// 打开剪贴板历史 (P2)
    OpenHistory,
    /// 切换 L1 滤镜
    ToggleFilter(FilterId),
    /// 打开偏好设置 (P2)
    OpenPreferences,
    /// 打开诊断看板 (P2)
    OpenDiagnostics,
    /// 退出 (触发生命周期物化)
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterId {
    Markdown,
    SocialLink,
}

/// MenuBar 控制中心状态机 (纯逻辑, 可测试)
pub struct MenuBarController {
    state: Mutex<MenuStateModel>,
}

#[derive(Debug)]
struct MenuStateModel {
    enabled: bool,
    filter_markdown: bool,
    filter_social_link: bool,
}

impl Default for MenuBarController {
    fn default() -> Self {
        Self::new()
    }
}

impl MenuBarController {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(MenuStateModel {
                enabled: true,
                filter_markdown: true,
                filter_social_link: true,
            }),
        }
    }

    /// 生成当前菜单定义 (UI 层渲染 NSMenu, IPC Spec §3.1 MenuBar Menu Definition)
    ///
    /// 9 项: 启用 + 分隔 + 2 滤镜 + 分隔 + 历史 + 诊断 + 分隔 + 退出, 全部启用。
    /// `Preferences` 暂不出现 (预留未来设置页)。
    pub fn menu_definition(&self) -> Vec<MenuItemDef> {
        let s = self.state.lock();
        vec![
            MenuItemDef::item(
                MenuItemId::EnableToggle,
                "启用 PasteFlow",
                true,
                if s.enabled { MenuState::On } else { MenuState::Off },
            ),
            MenuItemDef::separator(),
            MenuItemDef::item(
                MenuItemId::FilterMarkdown,
                "Markdown → 富文本",
                true,
                if s.filter_markdown { MenuState::On } else { MenuState::Off },
            ),
            MenuItemDef::item(
                MenuItemId::FilterSocialLink,
                "社媒链接净化",
                true,
                if s.filter_social_link { MenuState::On } else { MenuState::Off },
            ),
            MenuItemDef::separator(),
            MenuItemDef::item(MenuItemId::History, "打开剪贴板历史", true, MenuState::Off),
            MenuItemDef::item(MenuItemId::Diagnostics, "诊断看板", true, MenuState::Off),
            MenuItemDef::separator(),
            MenuItemDef::item(MenuItemId::Quit, "退出 PasteFlow", true, MenuState::Off),
        ]
    }

    /// 处理菜单动作, 返回是否需要触发生命周期物化
    pub fn handle_action(&self, action: MenuAction) -> bool {
        let mut s = self.state.lock();
        match action {
            MenuAction::ToggleEnabled => {
                s.enabled = !s.enabled;
                // 暂停时需物化当前接管
                !s.enabled
            }
            MenuAction::ToggleFilter(FilterId::Markdown) => {
                s.filter_markdown = !s.filter_markdown;
                false
            }
            MenuAction::ToggleFilter(FilterId::SocialLink) => {
                s.filter_social_link = !s.filter_social_link;
                false
            }
            MenuAction::Quit => {
                s.enabled = false;
                true // 触发生命周期物化
            }
            // 打开窗口类动作: 仅物化/展示目标窗口, 不触发生命周期物化
            MenuAction::OpenHistory | MenuAction::OpenPreferences | MenuAction::OpenDiagnostics => {
                false
            }
        }
    }

    /// 当前接管状态 (供轮询线程判断是否需启动/停止)
    pub fn is_enabled(&self) -> bool {
        self.state.lock().enabled
    }

    /// L1 滤镜启用状态 (供 PipeExecutor 判断)
    pub fn is_filter_enabled(&self, filter: FilterId) -> bool {
        let s = self.state.lock();
        match filter {
            FilterId::Markdown => s.filter_markdown,
            FilterId::SocialLink => s.filter_social_link,
        }
    }
}

/// 共享句柄
pub type SharedMenuBar = Arc<MenuBarController>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_definition_matches_ipc_spec() {
        let m = MenuBarController::new();
        let defs = m.menu_definition();
        // 9 项: 启用 + 分隔 + 2滤镜 + 分隔 + 历史 + 诊断 + 分隔 + 退出
        assert_eq!(defs.len(), 9);
        assert_eq!(defs[0].id, MenuItemId::EnableToggle);
        assert_eq!(defs[0].state, MenuState::On); // 默认启用
        assert!(defs[1].is_separator);
        assert_eq!(defs[2].id, MenuItemId::FilterMarkdown);
        assert_eq!(defs[3].id, MenuItemId::FilterSocialLink);
        assert!(defs[4].is_separator);
        assert_eq!(defs[5].id, MenuItemId::History);
        assert_eq!(defs[6].id, MenuItemId::Diagnostics);
        assert!(defs[7].is_separator);
        assert_eq!(defs[8].id, MenuItemId::Quit);
        // 全部启用 (与托盘菜单一致)
        for d in &defs {
            if !d.is_separator {
                assert!(d.enabled, "{:?} 应为启用", d.id);
            }
        }
    }

    #[test]
    fn toggle_enabled_flips_state_and_signals_materialize() {
        let m = MenuBarController::new();
        assert!(m.is_enabled());
        let needs_materialize = m.handle_action(MenuAction::ToggleEnabled);
        assert!(needs_materialize); // 暂停 -> 需物化
        assert!(!m.is_enabled());
        let needs2 = m.handle_action(MenuAction::ToggleEnabled);
        assert!(!needs2); // 启用 -> 无需物化
        assert!(m.is_enabled());
    }

    #[test]
    fn toggle_filter_independent() {
        let m = MenuBarController::new();
        assert!(m.is_filter_enabled(FilterId::Markdown));
        assert!(m.is_filter_enabled(FilterId::SocialLink));
        m.handle_action(MenuAction::ToggleFilter(FilterId::Markdown));
        assert!(!m.is_filter_enabled(FilterId::Markdown));
        assert!(m.is_filter_enabled(FilterId::SocialLink)); // 独立
    }

    #[test]
    fn quit_triggers_materialize() {
        let m = MenuBarController::new();
        let needs = m.handle_action(MenuAction::Quit);
        assert!(needs);
        assert!(!m.is_enabled());
    }

    #[test]
    fn p2_actions_noop() {
        let m = MenuBarController::new();
        assert!(!m.handle_action(MenuAction::OpenHistory));
        assert!(!m.handle_action(MenuAction::OpenPreferences));
        assert!(!m.handle_action(MenuAction::OpenDiagnostics));
    }

    #[test]
    fn menu_state_reflects_enabled() {
        let m = MenuBarController::new();
        let defs = m.menu_definition();
        assert_eq!(defs[0].state, MenuState::On);
        m.handle_action(MenuAction::ToggleEnabled);
        let defs2 = m.menu_definition();
        assert_eq!(defs2[0].state, MenuState::Off);
    }
}
