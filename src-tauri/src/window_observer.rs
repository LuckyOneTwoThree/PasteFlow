//! P1-2 WindowObserver — TDD §3.1
//!
//! 职责:
//! 1. 线程 A: 50ms 轮询 frontmost 进程, 写入 16 槽环形缓冲 (lock-free 语义, 见下)
//! 2. 回调线程: `frontmost()` O(1) 读取最外层元素
//! 3. `at(timestamp_ms)` 抗竞态回溯: ⌘V 回调晚于窗口焦点变化抵达时, 回溯本次复制事件
//!    时刻的活跃窗口, 避免误路由
//!
//! 实现注记: 用 parking_lot::Mutex 保护 ring 与 hot cache; O(1) 前台读取在锁内仅一次
//! 拷贝小 struct (AppIdentity 为两个 Arc<str> + bundle id), 实测 P0 中 FrontmostApp
//! 查询仅 0.042µs, 该锁不构成热路径瓶颈 (§1.4 线程模型允许受控锁)。

use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// 环形缓冲槽位数 (TDD §3.1: 16 槽 @ 50ms 轮询 = 800ms 回溯窗口)
pub const RING_CAPACITY: usize = 16;
/// 前台应用轮询周期 (TDD §3.1)
pub const POLL_INTERVAL_MS: u64 = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppIdentity {
    pub bundle_id: Arc<str>,
    pub localized_name: Arc<str>,
    pub pid: i32,
}

impl AppIdentity {
    pub fn new(bundle_id: impl Into<String>, localized_name: impl Into<String>, pid: i32) -> Self {
        Self {
            bundle_id: Arc::from(bundle_id.into().as_str()),
            localized_name: Arc::from(localized_name.into().as_str()),
            pid,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusEntry {
    /// Unix 毫秒 (单调性由轮询顺序保证)
    pub timestamp_ms: u64,
    pub app: AppIdentity,
}

#[derive(Debug)]
struct RingState {
    buf: Vec<Option<FocusEntry>>,
    head: usize, // 下一写入位置
    len: usize,
    frontmost: Option<AppIdentity>, // hot cache: 最新焦点
}

impl RingState {
    fn new() -> Self {
        Self {
            buf: (0..RING_CAPACITY).map(|_| None).collect(),
            head: 0,
            len: 0,
            frontmost: None,
        }
    }

    fn push(&mut self, entry: FocusEntry) {
        self.frontmost = Some(entry.app.clone());
        self.buf[self.head] = Some(entry);
        self.head = (self.head + 1) % RING_CAPACITY;
        if self.len < RING_CAPACITY {
            self.len += 1;
        }
    }

    /// 按时间倒序遍历 (最新在前)
    fn iter_reverse(&self) -> impl Iterator<Item = &FocusEntry> {
        (0..self.len).filter_map(move |i| {
            let idx = (self.head + RING_CAPACITY - 1 - i) % RING_CAPACITY;
            self.buf[idx].as_ref()
        })
    }

    /// 抗竞态回溯: 找 <= timestamp_ms 的最近焦点条目 (TDD §3.1)
    /// 兜底: 若请求时刻早于最旧条目, 返回最旧条目 (避免 None 导致路由缺失)
    fn at(&self, timestamp_ms: u64) -> Option<&FocusEntry> {
        self.iter_reverse()
            .find(|e| e.timestamp_ms <= timestamp_ms)
            .or_else(|| self.iter_reverse().last())
    }
}

/// Thread-safe WindowObserver
#[derive(Debug)]
pub struct WindowObserver {
    state: Mutex<RingState>,
    start: Instant,
}

impl Default for WindowObserver {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowObserver {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(RingState::new()),
            start: Instant::now(),
        }
    }

    /// 当前相对毫秒 (用于给外部时间源打点, 测试可注入)
    pub fn now_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }

    /// 系统 Unix 毫秒
    pub fn unix_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_millis() as u64
    }

    /// 由轮询线程写入
    pub fn record_focus(&self, timestamp_ms: u64, app: AppIdentity) {
        self.state.lock().push(FocusEntry {
            timestamp_ms,
            app,
        });
    }

    /// O(1) 前台应用读取 (Provider 回调热路径)
    pub fn frontmost(&self) -> Option<AppIdentity> {
        self.state.lock().frontmost.clone()
    }

    /// 抗竞态回溯: 返回 timestamp_ms 时刻的有效焦点
    pub fn at(&self, timestamp_ms: u64) -> Option<AppIdentity> {
        self.state.lock().at(timestamp_ms).map(|e| e.app.clone())
    }

    /// 快照 (诊断看板 / 单测)
    pub fn snapshot(&self) -> Vec<FocusEntry> {
        self.state
            .lock()
            .iter_reverse()
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str) -> AppIdentity {
        AppIdentity::new(format!("com.test.{name}"), name, 100)
    }

    #[test]
    fn ring_caps_at_16_and_drops_oldest() {
        let w = WindowObserver::new();
        for i in 0..20u64 {
            w.record_focus(i, app(&format!("a{i}")));
        }
        let snap = w.snapshot();
        assert_eq!(snap.len(), RING_CAPACITY);
        // 倒序第一条是最新的 a19
        assert_eq!(snap[0].app.bundle_id.as_ref(), "com.test.a19");
        // 最旧保留 a4 (20 写入, 16 槽)
        assert_eq!(snap.last().unwrap().app.bundle_id.as_ref(), "com.test.a4");
    }

    #[test]
    fn frontmost_tracks_latest() {
        let w = WindowObserver::new();
        w.record_focus(1, app("x"));
        w.record_focus(2, app("y"));
        assert_eq!(w.frontmost().unwrap().bundle_id.as_ref(), "com.test.y");
    }

    #[test]
    fn at_backtracks_to_history_entry() {
        let w = WindowObserver::new();
        w.record_focus(100, app("word"));
        w.record_focus(200, app("wechat"));
        // ⌘V 回调晚于焦点变化抵达: 回溯到复制时刻 150ms 的前台 = word
        assert_eq!(w.at(150).unwrap().bundle_id.as_ref(), "com.test.word");
        assert_eq!(w.at(250).unwrap().bundle_id.as_ref(), "com.test.wechat");
        // 早于最旧条目 -> 兜底取最旧
        assert_eq!(w.at(50).unwrap().bundle_id.as_ref(), "com.test.word");
    }

    #[test]
    fn empty_observer_returns_none() {
        let w = WindowObserver::new();
        assert!(w.frontmost().is_none());
        assert!(w.at(123).is_none());
    }
}
