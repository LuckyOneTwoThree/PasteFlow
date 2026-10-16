//! 测试门禁 3/3: hotkey_registry — TDD §2.3 / Dev Plan P1-14
//!
//! 热键注册表门禁:
//!   1. 双击 ⌘C 手势 (D4) 窗口约束 250ms
//!   2. Opt+1..4 槽位热键与 macOS 系统热键零冲突
//!   3. Escape / Ctrl+[ 链 30ms 原生优先
//!   4. 注册表内部无重复

use pasteflow_lib::double_copy::{CopyEventKind, DoubleCopyDetector};
use pasteflow_lib::hotkeys::{
    assert_activation_forbidden_free, assert_activation_unique, assert_no_conflicts, assert_unique,
    ACTIVATION_HOTKEYS, DOUBLE_CLICK_WINDOW_MS, FORBIDDEN_HOTKEYS, HUD_CTRL_BRACKET, HUD_ESCAPE,
    KNOWN_SYSTEM_CONFLICTS, SLOT_HOTKEYS,
};
use std::time::Duration;

// —— TDD §6.3: 5 个激活快捷键两两不等且不入禁用黑名单 ——

#[test]
fn activation_hotkeys_traverse_all_five() {
    // TDD §6.3 原文: 遍历 hotkeys.rs 定义的全部 5 个激活快捷键
    // (⌘⇧P / ⌃⌥V / ⌃⌥D / ⌥⇧V / ⌥⇧S), 断言任意两个键位组合两两不相等
    assert_eq!(ACTIVATION_HOTKEYS.len(), 5);
    assert!(assert_activation_unique());
}

#[test]
fn activation_hotkeys_avoid_forbidden_blacklist() {
    // TDD §6.3: 无任何键位落入 FORBIDDEN_HOTKEYS (⌥V / ⌘V⌘V)
    assert!(!FORBIDDEN_HOTKEYS.is_empty());
    assert!(assert_activation_forbidden_free());
}

#[test]
fn activation_hotkeys_avoid_system_conflicts() {
    let registered: Vec<String> = ACTIVATION_HOTKEYS.iter().map(|h| h.plugin.to_string()).collect();
    assert!(assert_no_conflicts(&registered).is_empty());
}

// —— D4 双击 ⌘C 手势 ——

#[test]
fn double_click_gesture_window_is_250ms() {
    assert_eq!(DOUBLE_CLICK_WINDOW_MS, 250);
}

#[test]
fn double_click_gesture_detected_via_double_copy() {
    // D4: 双击 ⌘C 由 DoubleCopyDetector 序列识别 (非注册式热键)
    let d = DoubleCopyDetector::new();
    assert_eq!(d.on_copy(777, true), CopyEventKind::Normal);
    std::thread::sleep(Duration::from_millis(80));
    assert_eq!(d.on_copy(777, true), CopyEventKind::DoubleClick); // 80ms 连按 -> 触发双击
    std::thread::sleep(Duration::from_millis(500));
    let third = d.on_copy(777, true);
    // 500ms > 450ms 双击窗口 -> 恢复常规单次复制
    assert_eq!(third, CopyEventKind::Normal);
}

#[test]
fn triple_copy_does_not_cascade() {
    let d = DoubleCopyDetector::new();
    d.on_copy(777, true);
    d.on_copy(777, true); // Debounced
    let third = d.on_copy(777, true);
    // 第三次仍走消抖合并, 不会连环触发双击
    assert_eq!(third, CopyEventKind::Debounced);
}

// —— Opt+1..4 槽位热键 ——

#[test]
fn slot_hotkeys_defined_four() {
    assert_eq!(SLOT_HOTKEYS.len(), 4);
    let names: Vec<&str> = SLOT_HOTKEYS.iter().map(|(k, _)| *k).collect();
    assert_eq!(names, vec!["Opt+1", "Opt+2", "Opt+3", "Opt+4"]);
}

#[test]
fn slot_hotkeys_no_system_conflicts() {
    let registered: Vec<String> = SLOT_HOTKEYS.iter().map(|(k, _)| k.to_string()).collect();
    assert!(assert_no_conflicts(&registered).is_empty());
    assert!(assert_unique(&registered));
}

#[test]
fn known_system_conflicts_nonempty_guard() {
    // 守护: 已知冲突集本身必须非空 (防误清空导致检测失效)
    assert!(!KNOWN_SYSTEM_CONFLICTS.is_empty());
    assert!(KNOWN_SYSTEM_CONFLICTS.contains(&"Cmd+Space"));
}

#[test]
fn conflicting_hotkey_rejected() {
    let registered = vec![
        "Ctrl+Up".to_string(),
        "Ctrl+Space".to_string(),
        "Opt+1".to_string(),
    ];
    let conflicts = assert_no_conflicts(&registered);
    assert_eq!(conflicts.len(), 2);
    assert!(conflicts.contains(&"Ctrl+Up".to_string()));
    assert!(conflicts.contains(&"Ctrl+Space".to_string()));
}

// —— Escape / Ctrl+[ 链 ——

#[test]
fn hud_escape_chain_defined() {
    assert_eq!(HUD_ESCAPE, "Escape");
    assert_eq!(HUD_CTRL_BRACKET, "Ctrl+[");
    // 二者不属于系统冲突集 (30ms 原生优先链, 不注册为全局热键)
    let chain = vec![HUD_ESCAPE.to_string(), HUD_CTRL_BRACKET.to_string()];
    assert!(assert_no_conflicts(&chain).is_empty());
}

// —— 注册表完整性 ——

#[test]
fn registry_no_duplicates_when_all_registered() {
    let mut all: Vec<String> = SLOT_HOTKEYS.iter().map(|(k, _)| k.to_string()).collect();
    all.push(HUD_ESCAPE.to_string());
    all.push(HUD_CTRL_BRACKET.to_string());
    assert!(assert_unique(&all));
    assert!(assert_no_conflicts(&all).is_empty());
}
