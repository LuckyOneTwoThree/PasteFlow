//! P1-5 CallbackBudget — TDD §3.5 / §1.4 线程模型
//!
//! Provider 回调硬预算:
//!   1. catch_unwind 防止 panic 穿过 FFI 边界 (未定义行为)
//!   2. 8ms 硬预算监控 (静默降级: 超预算记 warning, 绝不 panic)
//!   3. 正则等昂贵算子必须预编译 (DLP 走 O(1) 查表, 严禁 NSDataDetector)

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{Duration, Instant};

/// Provider 回调硬预算 (Master Spec §8.3 铁律 R1)
pub const BUDGET: Duration = Duration::from_millis(8);

/// 回调执行结果
#[derive(Debug)]
pub struct BudgetOutcome<T> {
    pub value: Option<T>,
    pub elapsed: Duration,
    pub panicked: bool,
    pub over_budget: bool,
}

/// 在 catch_unwind + 预算监控下执行回调体
pub fn run_with_budget<F, T>(f: F) -> BudgetOutcome<T>
where
    F: FnOnce() -> Option<T>,
{
    let start = Instant::now();
    let result = catch_unwind(AssertUnwindSafe(f));
    let elapsed = start.elapsed();
    match result {
        Ok(value) => {
            if elapsed > BUDGET {
                tracing::warn!(
                    target: "pasteflow_lib::budget",
                    elapsed_ms = elapsed.as_micros() as f64 / 1000.0,
                    "callback exceeded 8ms budget (silent degrade)"
                );
            }
            BudgetOutcome {
                value,
                elapsed,
                panicked: false,
                over_budget: elapsed > BUDGET,
            }
        }
        Err(_) => {
            // panic 被吞: 降级返回 None (AppleScript 友好), 绝不向 FFI 抛出
            tracing::error!(target: "pasteflow_lib::budget", "callback panicked, degraded to None");
            BudgetOutcome {
                value: None,
                elapsed,
                panicked: true,
                over_budget: true,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_execution_within_budget() {
        let out = run_with_budget(|| Some(42));
        assert_eq!(out.value, Some(42));
        assert!(!out.panicked);
        assert!(!out.over_budget);
    }

    #[test]
    fn panic_is_caught_and_degraded() {
        let out = run_with_budget(|| -> Option<i32> { panic!("boom") });
        assert!(out.panicked);
        assert_eq!(out.value, None);
    }

    #[test]
    fn over_budget_flagged() {
        let out = run_with_budget(|| {
            std::thread::sleep(Duration::from_millis(12));
            Some(())
        });
        assert!(out.over_budget);
        assert!(out.elapsed >= BUDGET);
    }
}
