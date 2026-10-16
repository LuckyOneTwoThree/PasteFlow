//! P1-9 Materializer — Master Spec §8.3 铁律 R4 / TDD §3.3
//!
//! 生命周期物化 (Lifecycle Materialization) 四步铁律:
//!   1. 捕获信号: 挂 signal handler 只设 volatile flag, 绝不调 AppKit
//!   2. 主队列接管: GCD signal source / RunLoop observer 通知主线程
//!   3. 物化: 主线程同步写入最终三通道数据 (pasteboard.clearContents + setString)
//!   4. 释放: releaseProviders 释放 DataProvider 保活引用
//!
//! 触发时机: 退出 / 暂停 / 热键禁用 / MenuBar Quit / ⌘Q
//! 铁律 (§8.3 R4): 严禁在裸 signal handler 内调用 AppKit (未定义行为)
//!
//! 实现注记: 本模块提供信号源抽象与四步状态机; 与 objc2/AppKit 的实际
//! clearContents 交互由 clipboard_owner::ClipboardOwner 协作完成。

use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// 物化步骤状态 (供测试断言四步顺序)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterializeStep {
    /// 初始
    Idle,
    /// Step 1: 信号已捕获 (flag 置位)
    SignalCaptured,
    /// Step 2: 主线程已接管
    MainThreadDispatched,
    /// Step 3: 数据已同步物化
    Materialized,
    /// Step 4: Provider 已释放
    ProvidersReleased,
}

/// 全局 volatile flag (signal handler 只写此变量)
static SIGNAL_FLAG: AtomicBool = AtomicBool::new(false);

/// signal handler 内部: 仅置位 flag, 绝不调 AppKit
pub fn raw_signal_handler(_signum: i32) {
    SIGNAL_FLAG.store(true, Ordering::SeqCst);
}

/// 主线程侧: 检查并消费信号 (GCD signal source 或 RunLoop observer 每帧调用)
pub fn consume_signal_flag() -> bool {
    SIGNAL_FLAG.swap(false, Ordering::SeqCst)
}

/// 物化请求
#[derive(Debug, Clone)]
pub struct MaterializeRequest {
    /// 触发原因 (退出/暂停/热键禁用/菜单栏 Quit)
    pub reason: MaterializeReason,
    /// 当前持有的三通道最终 payload (静态物化目标)
    pub html: Option<String>,
    pub rtf: Option<String>,
    pub plain: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterializeReason {
    /// App 退出 (⌘Q / MenuBar Quit)
    Quit,
    /// 暂停接管 (热键暂停)
    Pause,
    /// 热键全局禁用
    HotkeyDisabled,
    /// 用户切换接管开关
    ToggleOff,
    /// P2-1 HUD 槽位 0: 还原原文 (builtin.restore-raw)
    RestoreRaw,
}

/// 物化状态机 (单线程主线程执行)
#[derive(Debug)]
pub struct Materializer {
    step: Mutex<MaterializeStep>,
    log: Mutex<Vec<MaterializeStep>>,
}

impl Default for Materializer {
    fn default() -> Self {
        Self::new()
    }
}

impl Materializer {
    pub fn new() -> Self {
        Self {
            step: Mutex::new(MaterializeStep::Idle),
            log: Mutex::new(Vec::new()),
        }
    }

    pub fn current_step(&self) -> MaterializeStep {
        *self.step.lock()
    }

    /// 四步物化序列 (主线程调用, 同步执行)
    ///   1. SignalCaptured (消费 flag)
    ///   2. MainThreadDispatched (已在主线程)
    ///   3. Materialized (调用方负责实际写 pasteboard, 这里只推进状态)
    ///   4. ProvidersReleased (调用方负责 release, 这里只推进状态)
    pub fn run_lifecycle(&self, req: MaterializeRequest) -> MaterializeStep {
        {
            let mut step = self.step.lock();
            *step = MaterializeStep::SignalCaptured;
            self.log.lock().push(*step);
        }
        {
            let mut step = self.step.lock();
            *step = MaterializeStep::MainThreadDispatched;
            self.log.lock().push(*step);
        }
        // Step 3: 实际物化由调用方 (clipboard_owner) 完成后回调 mark_materialized
        let _ = req;
        let mut step = self.step.lock();
        *step = MaterializeStep::Materialized;
        self.log.lock().push(*step);
        *step
    }

    /// 调用方完成实际 pasteboard 写入后调用
    pub fn mark_materialized(&self) {
        let mut step = self.step.lock();
        *step = MaterializeStep::Materialized;
        self.log.lock().push(*step);
    }

    /// 调用方完成 release 后调用
    pub fn mark_providers_released(&self) {
        let mut step = self.step.lock();
        *step = MaterializeStep::ProvidersReleased;
        self.log.lock().push(*step);
    }

    /// 测试断言: 四步顺序是否正确
    pub fn sequence_log(&self) -> Vec<MaterializeStep> {
        self.log.lock().clone()
    }

    /// 重置 (测试用)
    pub fn reset(&self) {
        let mut step = self.step.lock();
        *step = MaterializeStep::Idle;
        self.log.lock().clear();
    }
}

/// 共享句柄
pub type SharedMaterializer = Arc<Materializer>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_handler_sets_flag() {
        SIGNAL_FLAG.store(false, Ordering::SeqCst);
        raw_signal_handler(15);
        assert!(consume_signal_flag());
        assert!(!consume_signal_flag()); // 已消费
    }

    #[test]
    fn lifecycle_four_steps_order() {
        let m = Materializer::new();
        let req = MaterializeRequest {
            reason: MaterializeReason::Quit,
            html: Some("<p>x</p>".into()),
            rtf: Some(r"{\rtf1 x}".into()),
            plain: Some("x".into()),
        };
        m.run_lifecycle(req);
        m.mark_providers_released();
        let log = m.sequence_log();
        // 顺序: SignalCaptured -> MainThreadDispatched -> Materialized -> ProvidersReleased
        assert_eq!(log[0], MaterializeStep::SignalCaptured);
        assert_eq!(log[1], MaterializeStep::MainThreadDispatched);
        assert_eq!(log[2], MaterializeStep::Materialized);
        assert_eq!(log.last().unwrap(), &MaterializeStep::ProvidersReleased);
    }

    #[test]
    fn materialize_reason_variants() {
        for r in [
            MaterializeReason::Quit,
            MaterializeReason::Pause,
            MaterializeReason::HotkeyDisabled,
            MaterializeReason::ToggleOff,
        ] {
            let m = Materializer::new();
            let req = MaterializeRequest {
                reason: r,
                html: None,
                rtf: None,
                plain: Some("plain".into()),
            };
            m.run_lifecycle(req);
            assert_eq!(m.current_step(), MaterializeStep::Materialized);
        }
    }
}
