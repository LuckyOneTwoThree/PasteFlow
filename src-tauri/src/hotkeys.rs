//! P1-13 hotkeys — TDD §2.3 / Master Spec §3.2 (D4 双击 ⌘C⌘C, Opt+1..4 槽位切换)
//!
//! 双击 ⌘C⌘C 是「手势」而非热键: 由 DoubleCopyDetector 识别, 不进此表。
//! 本表管理三类热键:
//!   1. 全局双击 ⌘C (通过 changeCount 序列检测, 非注册式热键)
//!   2. 系统热键冲突断言 (Opt+1..4 与 Stage Manager / 输入法等冲突检测)
//!   3. Escape / Ctrl+[ 链 (HUD 关闭, 30ms 原生优先)

use std::collections::HashSet;

/// HUD/Toast 快捷键 (原生捕获优先, 30ms 内命中)
pub const HUD_ESCAPE: &str = "Escape";
pub const HUD_CTRL_BRACKET: &str = "Ctrl+[";

/// 槽位切换热键 (D4: Opt+1..4 = 顶部 4 个历史快照)
pub const SLOT_HOTKEYS: &[(&str, &str)] = &[
    ("Opt+1", "slot_1"),
    ("Opt+2", "slot_2"),
    ("Opt+3", "slot_3"),
    ("Opt+4", "slot_4"),
];

/// 双击 ⌘C 手势检测窗口 (master spec §3.2)
pub const DOUBLE_CLICK_WINDOW_MS: u64 = 250;

// —— 缺口1: 全局激活热键登记表 (TDD §4.5 / Master Spec §4.5, 5 个) ——

/// 激活热键登记表条目: (动作 ID, 显示符号, tauri-plugin-global-shortcut 串)
pub struct ActivationHotkey {
    pub id: &'static str,
    pub display: &'static str,
    /// tauri-plugin-global-shortcut 可解析的键位串 (如 "command+shift+p")
    pub plugin: &'static str,
}

/// 5 个激活快捷键 (TDD §6.3 门禁要求两两不等且不入禁用黑名单):
///   ⌘⇧P 自动挡暂停/恢复 | ⌃⌥V 手动挡备选热键 | ⌃⌥D 显式 DLP 脱敏
///   ⌥⇧V 历史与贪吃蛇抽屉 | ⌥⇧S 贪吃蛇模式开关
pub const ACTIVATION_HOTKEYS: &[ActivationHotkey] = &[
    ActivationHotkey { id: "autopilot_pause", display: "⌘⇧P", plugin: "command+shift+p" },
    ActivationHotkey { id: "manual_summon", display: "⌃⌥V", plugin: "ctrl+alt+v" },
    ActivationHotkey { id: "dlp_redact", display: "⌃⌥D", plugin: "ctrl+alt+d" },
    ActivationHotkey { id: "drawer_history", display: "⌥⇧V", plugin: "shift+alt+v" },
    ActivationHotkey { id: "stack_mode", display: "⌥⇧S", plugin: "shift+alt+s" },
];

/// 禁用黑名单 (TDD §6.3): 与原生粘贴冲突 (⌥V=√ 字符) / 与 ⌘C⌘C 手势混淆
pub const FORBIDDEN_HOTKEYS: &[&str] = &["⌥V", "⌘V⌘V"];

/// 5 个激活热键两两不等 (TDD §6.3 门禁)
pub fn assert_activation_unique() -> bool {
    let mut seen = HashSet::new();
    ACTIVATION_HOTKEYS
        .iter()
        .all(|h| seen.insert(h.plugin) && seen.insert(h.display))
}

/// 激活热键无一落入禁用黑名单 (TDD §6.3 门禁)
pub fn assert_activation_forbidden_free() -> bool {
    ACTIVATION_HOTKEYS
        .iter()
        .all(|h| !FORBIDDEN_HOTKEYS.contains(&h.display) && !FORBIDDEN_HOTKEYS.contains(&h.plugin))
}

/// 已知系统热键冲突 (Stage Manager / 输入法 / 全屏快捷键)
pub const KNOWN_SYSTEM_CONFLICTS: &[&str] = &[
    "Ctrl+Up",     // Mission Control (macOS)
    "Ctrl+Down",   // Application Windows
    "Cmd+Space",   // Spotlight
    "Ctrl+Space",  // 输入法切换
    "Cmd+Tab",     // App 切换器
];

/// 热键无冲突断言: 检查注册热键集与系统冲突集不相交
pub fn assert_no_conflicts(registered: &[String]) -> Vec<String> {
    let system: HashSet<&str> = KNOWN_SYSTEM_CONFLICTS.iter().copied().collect();
    registered
        .iter()
        .filter(|k| system.contains(k.as_str()))
        .cloned()
        .collect()
}

/// 检查热键集合内部无重复
pub fn assert_unique(registered: &[String]) -> bool {
    let mut seen = HashSet::new();
    registered.iter().all(|k| seen.insert(k.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_conflicts_when_clean() {
        let registered = vec!["Opt+1".to_string(), "Opt+2".to_string(), "Escape".to_string()];
        let conflicts = assert_no_conflicts(&registered);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn detects_system_conflict() {
        let registered = vec!["Ctrl+Up".to_string(), "Opt+1".to_string()];
        let conflicts = assert_no_conflicts(&registered);
        assert_eq!(conflicts, vec!["Ctrl+Up"]);
    }

    #[test]
    fn unique_hotkeys_pass() {
        let registered = vec!["Opt+1".to_string(), "Opt+2".to_string()];
        assert!(assert_unique(&registered));
    }

    #[test]
    fn duplicate_hotkeys_fail() {
        let registered = vec!["Opt+1".to_string(), "Opt+1".to_string()];
        assert!(!assert_unique(&registered));
    }

    #[test]
    fn slot_hotkeys_count() {
        assert_eq!(SLOT_HOTKEYS.len(), 4);
    }

    #[test]
    fn activation_hotkeys_are_five_and_unique() {
        // TDD §6.3: 遍历全部 5 个激活快捷键 (⌘⇧P / ⌃⌥V / ⌃⌥D / ⌥⇧V / ⌥⇧S)
        assert_eq!(ACTIVATION_HOTKEYS.len(), 5);
        assert!(assert_activation_unique());
    }

    #[test]
    fn activation_hotkeys_avoid_forbidden_blacklist() {
        // TDD §6.3: 无键位落入禁用黑名单 (⌥V / ⌘V⌘V)
        assert!(assert_activation_forbidden_free());
    }

    #[test]
    fn activation_hotkeys_match_master_spec_symbols() {
        let displays: Vec<&str> = ACTIVATION_HOTKEYS.iter().map(|h| h.display).collect();
        assert_eq!(displays, vec!["⌘⇧P", "⌃⌥V", "⌃⌥D", "⌥⇧V", "⌥⇧S"]);
    }
}
