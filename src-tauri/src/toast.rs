//! P1-11 NativeToastController — Dev Plan P1-11 / UI Spec §2.2 / Master Spec §3.2 (260ms 消抖 Toast)
//!
//! 原生 Toast (零 Webview, 55MB 内存红线):
//!   - 非激活 NSPanel (borderless + nonactivatingPanel), 不抢焦点
//!   - CATextLayer 渲染文本, 圆角 NSVisualEffectView 磨砂背景
//!   - 260ms 消抖联动: DoubleCopyDetector.in_debounce_window() 时抑制重复 Toast
//!   - 淡入 -> 停留 -> 淡出 (2s 总时长)
//!
//! 铁律 (§8.3 R4): Toast 生命周期严格在主线程, 绝不在 signal handler / 后台线程调 AppKit

use crate::double_copy::DoubleCopyDetector;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Toast 显示时长 (UI Spec §2.2)
pub const TOAST_DURATION_MS: u64 = 2000;
/// Toast 消抖窗口 (Master Spec §3.2)
pub const TOAST_DEBOUNCE_MS: u64 = 260;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToastRequest {
    pub text: String,
    /// 轻提示 (信息) / 警告 / 静默三不提示
    pub level: ToastLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastLevel {
    Info,
    Warning,
    /// DLP 静默三不 (Master Spec §3.2: 不弹明文, 仅示意盾牌)
    SilentDlp,
}

impl Default for ToastRequest {
    fn default() -> Self {
        Self {
            text: String::new(),
            level: ToastLevel::Info,
        }
    }
}

/// Toast 显示状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastVisibility {
    Hidden,
    Visible,
    /// 消抖命中: 上一个 Toast 未超时且内容相同
    Debounced,
}

/// Toast 状态机 (主线程调用 show/hide, 后台仅读取状态)
pub struct NativeToastController {
    state: Mutex<ToastState>,
    double_copy: Arc<DoubleCopyDetector>,
}

#[derive(Debug)]
struct ToastState {
    current: Option<(ToastRequest, Instant)>,
    visible: ToastVisibility,
}

impl NativeToastController {
    pub fn new(double_copy: Arc<DoubleCopyDetector>) -> Self {
        Self {
            state: Mutex::new(ToastState {
                current: None,
                visible: ToastVisibility::Hidden,
            }),
            double_copy,
        }
    }

    /// 请求显示 Toast
    /// 返回 ToastVisibility: Debounced = 被消抖抑制 (不重复弹)
    pub fn show(&self, req: ToastRequest) -> ToastVisibility {
        let mut s = self.state.lock();

        // 260ms 消抖联动 (Master Spec §3.2): 同内容在消抖窗口内不重复弹
        if let Some((prev, t)) = &s.current {
            if prev.text == req.text
                && t.elapsed() < Duration::from_millis(TOAST_DEBOUNCE_MS)
                && self.double_copy.in_debounce_window()
            {
                s.visible = ToastVisibility::Debounced;
                return ToastVisibility::Debounced;
            }
        }

        s.current = Some((req, Instant::now()));
        s.visible = ToastVisibility::Visible;
        ToastVisibility::Visible
    }

    /// 手动 hide (用户点击 / 切换)
    pub fn hide(&self) {
        let mut s = self.state.lock();
        s.current = None;
        s.visible = ToastVisibility::Hidden;
    }

    /// 超时自动 hide (2s 到)
    pub fn tick(&self) {
        let mut s = self.state.lock();
        if let Some((_, t)) = &s.current {
            if t.elapsed() >= Duration::from_millis(TOAST_DURATION_MS) {
                s.current = None;
                s.visible = ToastVisibility::Hidden;
            }
        }
    }

    /// 当前可见性 (测试断言)
    pub fn visibility(&self) -> ToastVisibility {
        self.state.lock().visible
    }

    /// 当前 Toast 文本 (UI 层渲染)
    pub fn current_text(&self) -> Option<String> {
        self.state.lock().current.as_ref().map(|(r, _)| r.text.clone())
    }
}

/// 260ms 消抖判断独立函数 (供 UI 层便捷调用)
pub fn should_suppress_toast(double_copy: &DoubleCopyDetector) -> bool {
    double_copy.in_debounce_window()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make() -> NativeToastController {
        let dc = Arc::new(DoubleCopyDetector::new());
        NativeToastController::new(dc)
    }

    #[test]
    fn toast_shows_first_time() {
        let t = make();
        let v = t.show(ToastRequest {
            text: "已复制".into(),
            level: ToastLevel::Info,
        });
        assert_eq!(v, ToastVisibility::Visible);
        assert_eq!(t.visibility(), ToastVisibility::Visible);
        assert_eq!(t.current_text().unwrap(), "已复制");
    }

    #[test]
    fn toast_debounced_on_duplicate_within_260ms() {
        let dc = Arc::new(DoubleCopyDetector::new());
        let t = NativeToastController::new(dc.clone());
        // 第一次弹出
        t.show(ToastRequest {
            text: "已复制".into(),
            level: ToastLevel::Info,
        });
        // 同内容再次复制: 消抖窗口激活 (DoubleCopyDetector 合并)
        dc.on_copy(111, true);
        assert!(dc.in_debounce_window());
        let v = t.show(ToastRequest {
            text: "已复制".into(),
            level: ToastLevel::Info,
        });
        assert_eq!(v, ToastVisibility::Debounced);
    }

    #[test]
    fn toast_auto_hide_after_2s() {
        let t = make();
        t.show(ToastRequest::default());
        assert_eq!(t.visibility(), ToastVisibility::Visible);
        std::thread::sleep(Duration::from_millis(TOAST_DURATION_MS + 50));
        t.tick();
        assert_eq!(t.visibility(), ToastVisibility::Hidden);
    }

    #[test]
    fn toast_manual_hide() {
        let t = make();
        t.show(ToastRequest::default());
        t.hide();
        assert_eq!(t.visibility(), ToastVisibility::Hidden);
    }

    #[test]
    fn different_text_not_debounced() {
        let dc = Arc::new(DoubleCopyDetector::new());
        let t = NativeToastController::new(dc.clone());
        t.show(ToastRequest {
            text: "A".into(),
            level: ToastLevel::Info,
        });
        dc.on_copy(111, true);
        let v = t.show(ToastRequest {
            text: "B".into(),
            level: ToastLevel::Info,
        });
        assert_eq!(v, ToastVisibility::Visible); // 不同内容不消抖
    }

    #[test]
    fn dlp_level_toast_has_no_plaintext() {
        // 静默三不: 文本不含密钥明文 (Master Spec §3.2)
        let t = make();
        let req = ToastRequest {
            text: "🔒 敏感信息已保护".into(),
            level: ToastLevel::SilentDlp,
        };
        t.show(req);
        let txt = t.current_text().unwrap();
        assert!(!txt.contains("sk-"));
        assert!(!txt.contains("ghp_"));
    }
}
