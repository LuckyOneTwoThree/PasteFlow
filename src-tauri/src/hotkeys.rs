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
}
