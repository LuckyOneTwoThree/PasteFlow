//! P1-8 ClipboardOwner — TDD §3.3 / Master Spec §3.2
//!
//! 剪贴板所有权管理 (macOS NSPasteboard DataProvider):
//!   1. 50ms 轮询 changeCount, 检测外部复制写入
//!   2. TOCTOU 原子 CAS: changeCount 不变才 clearContents (防正被粘贴时剪贴板被外部写入)
//!   3. Provider 保活: NSPasteboardItemDataProvider 实例必须存活至 pasteboardFinished
//!   4. Re-own 补挂 (P0 exp5): Probe Guard 触发后全通道供给完成或 300ms 超时,
//!      补挂新一代 Provider (<=3 代, 补挂 changeCount 登记 InternalWrite)
//!
//! 铁律 (§8.3 R4): 退出/暂停时先物化 (Materializer), 严禁裸 signal handler 调 AppKit

use parking_lot::Mutex;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::Arc;

/// 轮询间隔 (TDD §3.1/§3.3)
pub const POLL_INTERVAL_MS: u64 = 50;

/// TOCTOU 校验结果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToctouResult {
    /// changeCount 未变, 可安全清空
    Safe,
    /// changeCount 已变 (外部并发写入), 放弃接管
    Aborted,
}

/// 一次接管的元数据 (供 Materializer / LoopGuard 登记)
#[derive(Debug, Clone)]
pub struct OwnershipRecord {
    pub change_count: i64,
    pub generation: usize,
    pub is_internal_write: bool,
    pub source_hash: u64,
}

/// Provider 保活容器 (iOS/macOS 要求 DataProvider 活到 pasteboardFinishedWithDataProvider)
/// 这里用 Box<dyn Any> 语义抽象, 实际 NSPasteboardItemDataProvider 由 objc2 层持有
pub struct ProviderKeeper {
    slots: Mutex<Vec<OwnedProviderSlot>>,
}

impl std::fmt::Debug for ProviderKeeper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderKeeper")
            .field("active_count", &self.slots.lock().len())
            .finish()
    }
}

struct OwnedProviderSlot {
    _keepalive: Box<dyn std::any::Any + Send>,
    _generation: usize,
}

impl Default for ProviderKeeper {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderKeeper {
    pub fn new() -> Self {
        Self {
            slots: Mutex::new(Vec::new()),
        }
    }

    /// 保活一个 provider 直到 pasteboardFinished
    pub fn keep(&self, provider: Box<dyn std::any::Any + Send>, generation: usize) {
        self.slots.lock().push(OwnedProviderSlot {
            _keepalive: provider,
            _generation: generation,
        });
    }

    /// pasteboardFinishedWithDataProvider 后释放 (但保留历史代号供诊断)
    pub fn release_finished(&self) {
        self.slots.lock().clear();
    }

    pub fn active_count(&self) -> usize {
        self.slots.lock().len()
    }
}

/// TOCTOU CAS: changeCount 前后一致性校验
pub fn toctou_check(before: i64, after: i64) -> ToctouResult {
    if before == after {
        ToctouResult::Safe
    } else {
        ToctouResult::Aborted
    }
}

/// ClipboardOwner 核心状态 (跨线程共享)
#[derive(Debug)]
pub struct ClipboardOwner {
    /// 当前 changeCount (原子, 轮询线程与回调线程共享)
    pub current_change: AtomicI64,
    /// 当前 Provider 代数 (Re-own 补挂递增)
    pub generation: AtomicUsize,
    /// 最近一次内部写入的 changeCount 登记 (LoopGuard.is_internal_write 判定)
    internal_writes: Mutex<Vec<i64>>,
    keeper: ProviderKeeper,
}

impl Default for ClipboardOwner {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardOwner {
    pub fn new() -> Self {
        Self {
            current_change: AtomicI64::new(0),
            generation: AtomicUsize::new(0),
            internal_writes: Mutex::new(Vec::new()),
            keeper: ProviderKeeper::new(),
        }
    }

    /// 轮询线程更新 changeCount (单调递增)
    pub fn observe_change(&self, change_count: i64) {
        let cur = self.current_change.load(Ordering::Acquire);
        if change_count > cur {
            self.current_change.store(change_count, Ordering::Release);
        }
    }

    /// 登记内部写入 (Re-own 补挂 / Materializer)
    pub fn register_internal_write(&self, change_count: i64) {
        let mut s = self.internal_writes.lock();
        s.push(change_count);
        if s.len() > 16 {
            s.remove(0);
        }
    }

    pub fn is_internal_write(&self, change_count: i64) -> bool {
        self.internal_writes.lock().contains(&change_count)
    }

    /// Re-own 补挂触发 (P0 exp5 / §1.4.2):
    /// 返回新代数; 达上限时返回 None (降级 StaticUnmanaged)
    /// max_generations = 补挂次数上限 (3 = 允许补挂 3 次)
    pub fn next_generation(&self, max_generations: usize) -> Option<usize> {
        let g = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        if g <= max_generations {
            Some(g)
        } else {
            None
        }
    }

    pub fn keep_provider(&self, provider: Box<dyn std::any::Any + Send>, generation: usize) {
        self.keeper.keep(provider, generation);
    }

    pub fn release_providers(&self) {
        self.keeper.release_finished();
    }

    pub fn provider_count(&self) -> usize {
        self.keeper.active_count()
    }

    /// 供 Materializer 调用: 物化后登记 (changeCount 通常 +1)
    pub fn materialize_snapshot(&self) -> OwnershipRecord {
        let cc = self.current_change.load(Ordering::Acquire);
        self.register_internal_write(cc);
        OwnershipRecord {
            change_count: cc,
            generation: self.generation.load(Ordering::Acquire),
            is_internal_write: true,
            source_hash: 0,
        }
    }
}

/// 共享句柄
pub type SharedOwner = Arc<ClipboardOwner>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toctou_safe_when_unchanged() {
        assert_eq!(toctou_check(5, 5), ToctouResult::Safe);
    }

    #[test]
    fn toctou_aborts_when_changed() {
        assert_eq!(toctou_check(5, 6), ToctouResult::Aborted);
    }

    #[test]
    fn change_count_monotonic() {
        let o = ClipboardOwner::new();
        o.observe_change(10);
        assert_eq!(o.current_change.load(Ordering::Acquire), 10);
        o.observe_change(5); // 非递增, 忽略
        assert_eq!(o.current_change.load(Ordering::Acquire), 10);
        o.observe_change(11);
        assert_eq!(o.current_change.load(Ordering::Acquire), 11);
    }

    #[test]
    fn internal_write_tracking() {
        let o = ClipboardOwner::new();
        o.register_internal_write(42);
        assert!(o.is_internal_write(42));
        assert!(!o.is_internal_write(43));
    }

    #[test]
    fn generation_increments_until_cap() {
        let o = ClipboardOwner::new();
        assert_eq!(o.next_generation(3), Some(1));
        assert_eq!(o.next_generation(3), Some(2));
        assert_eq!(o.next_generation(3), Some(3));
        assert_eq!(o.next_generation(3), None); // 达上限 (补挂 3 次后)
    }

    #[test]
    fn provider_keepalive_counts() {
        let o = ClipboardOwner::new();
        o.keep_provider(Box::new(1i32), 1);
        o.keep_provider(Box::new(2i32), 2);
        assert_eq!(o.provider_count(), 2);
        o.release_providers();
        assert_eq!(o.provider_count(), 0);
    }

    #[test]
    fn materialize_registers_internal_write() {
        let o = ClipboardOwner::new();
        o.observe_change(7);
        let rec = o.materialize_snapshot();
        assert!(rec.is_internal_write);
        assert!(o.is_internal_write(7));
    }
}
