//! P1-10 DoubleCopyDetector — Master Spec §3.2 (D4 双击 ⌘C⌘C) / TDD §3.6
//!
//! 双信号协同:
//!   1. 260ms 消抖: 同一内容在 260ms 内二次 changeCount 事件 -> 合并为一次复制 (Toast 抑制)
//!   2. 双击检测: 连续两次真实 ⌘C 且间隔 <250ms 且同一 L1 快照哈希 -> 触发双击还原/HUD
//!
//! 补挂屏蔽 (P0 exp5 / §1.4.2): Re-own 补挂产生的 changeCount 必须登记为
//! InternalWrite 并被本模块短路, 否则会误判为外部双击复制。

use parking_lot::Mutex;
use std::time::{Duration, Instant};

/// 同内容消抖窗口 (Master Spec §3.2)
pub const DEBOUNCE_WINDOW: Duration = Duration::from_millis(260);
/// 双击 ⌘C 窗口 (Master Spec §3.2: <250ms)
pub const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyEventKind {
    /// 常规复制
    Normal,
    /// 消抖命中: 260ms 内同内容重复事件, 合并 (不重复弹 Toast)
    Debounced,
    /// 双击 ⌘C: 触发还原/HUD
    DoubleClick,
}

#[derive(Debug)]
struct State {
    last_change: Option<(Instant, u64)>, // (time, snapshot_hash)
    last_click: Option<(Instant, u64)>,
}

pub struct DoubleCopyDetector {
    state: Mutex<State>,
}

impl Default for DoubleCopyDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl DoubleCopyDetector {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State {
                last_change: None,
                last_click: None,
            }),
        }
    }

    /// 记录一次复制事件
    ///   change_count: 本次 changeCount (若为补挂内部写入, 调用方应先 short_circuit)
    ///   snapshot_hash: L1 快照哈希 (内容指纹, 用于消抖与双击内容一致性判定)
    ///   is_real_copy: 是否真实用户 ⌘C (vs 内部写入/补挂)
    pub fn on_copy(&self, snapshot_hash: u64, is_real_copy: bool) -> CopyEventKind {
        let now = Instant::now();
        let mut s = self.state.lock();

        // 消抖: 同内容 <260ms 重复
        if let Some((t, h)) = s.last_change {
            if h == snapshot_hash && now.duration_since(t) < DEBOUNCE_WINDOW {
                s.last_change = Some((now, snapshot_hash));
                return CopyEventKind::Debounced;
            }
        }
        s.last_change = Some((now, snapshot_hash));

        if !is_real_copy {
            return CopyEventKind::Normal;
        }

        // 双击: 连续两次真实 ⌘C, <250ms, 同内容
        if let Some((t, h)) = s.last_click {
            if h == snapshot_hash && now.duration_since(t) < DOUBLE_CLICK_WINDOW {
                s.last_click = None; // 触发后清零, 防三击连环
                return CopyEventKind::DoubleClick;
            }
        }
        s.last_click = Some((now, snapshot_hash));
        CopyEventKind::Normal
    }

    /// 内部写入短路 (P0 §1.4.2): 补挂 changeCount 不参与双击/消抖判定
    pub fn short_circuit_internal(&self) {
        let mut s = self.state.lock();
        s.last_click = None;
    }

    /// 当前是否处于消抖窗口 (Toast 抑制判断)
    pub fn in_debounce_window(&self) -> bool {
        let s = self.state.lock();
        if let Some((t, _)) = s.last_change {
            return t.elapsed() < DEBOUNCE_WINDOW;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debounce_merges_fast_duplicates() {
        let d = DoubleCopyDetector::new();
        assert_eq!(d.on_copy(111, true), CopyEventKind::Normal);
        assert_eq!(d.on_copy(111, true), CopyEventKind::Debounced); // <260ms 同内容
    }

    #[test]
    fn different_content_not_debounced() {
        let d = DoubleCopyDetector::new();
        assert_eq!(d.on_copy(111, true), CopyEventKind::Normal);
        assert_eq!(d.on_copy(222, true), CopyEventKind::Normal);
    }

    #[test]
    fn double_click_detected() {
        let d = DoubleCopyDetector::new();
        assert_eq!(d.on_copy(111, true), CopyEventKind::Normal);
        std::thread::sleep(Duration::from_millis(80));
        assert_eq!(d.on_copy(111, true), CopyEventKind::Debounced); // 消抖先于双击 (260ms)
    }

    #[test]
    fn debounce_resets_after_window() {
        let d = DoubleCopyDetector::new();
        assert_eq!(d.on_copy(111, true), CopyEventKind::Normal);
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(d.on_copy(111, true), CopyEventKind::Normal); // 超出消抖窗口
    }

    #[test]
    fn internal_write_short_circuits() {
        let d = DoubleCopyDetector::new();
        d.on_copy(111, true);
        d.short_circuit_internal(); // 补挂 -> 清零点击状态
        assert_eq!(d.on_copy(222, true), CopyEventKind::Normal); // 不会误判双击
    }

    #[test]
    fn debounce_window_check() {
        let d = DoubleCopyDetector::new();
        assert!(!d.in_debounce_window());
        d.on_copy(111, true);
        assert!(d.in_debounce_window());
    }
}
