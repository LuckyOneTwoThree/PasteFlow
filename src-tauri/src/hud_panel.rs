//! P2-1 Micro-HUD 原生面板骨架 (UI Spec v1.0 §3.1 / TDD `non_activating_panel.rs`)
//!
//! `NonActivatingHUDPanel`：NSPanel 子类，`canBecomeKey = true`、`canBecomeMain = false`，
//! 用于在**不激活应用**的前提下接收 HUD 按键（P0 S8 实测属性校验通过）。
//!
//! 铁律：
//!   - 常驻隐藏不销毁（D1：启动 2s 预热，`visible=false`）
//!   - 按键 100% 不泄漏进宿主应用（收键后由状态机消费，绝不转发）
//!   - R-2 风险降级预案：WKWebView 收键异常时改由隐藏 NSTextField 承接按键转发

use objc2::declare_class;
use objc2::msg_send_id;
use objc2::mutability;
use objc2::rc::Retained;
use objc2::runtime::NSObjectProtocol;
use objc2::{ClassType, DeclaredClass};
use objc2_app_kit::NSPanel;
use objc2_foundation::MainThreadMarker;

// NonActivatingHUDPanel: NSPanel 子类
declare_class!(
    struct NonActivatingHUDPanel;

    // SAFETY:
    // - Super = NSPanel (MainThreadOnly)
    // - 只覆写 canBecomeKey/canBecomeMain 两个只读方法, 无可变状态
    // - 不实现 Drop
    unsafe impl ClassType for NonActivatingHUDPanel {
        type Super = NSPanel;
        type Mutability = mutability::MainThreadOnly;
        const NAME: &'static str = "NonActivatingHUDPanel";
    }

    impl DeclaredClass for NonActivatingHUDPanel {
        type Ivars = ();
    }

    unsafe impl NSObjectProtocol for NonActivatingHUDPanel {}

    unsafe impl NonActivatingHUDPanel {
        /// S8 实测: canBecomeKey=true (P0 S8 属性校验)
        #[method(canBecomeKey)]
        fn can_become_key(&self) -> bool {
            true
        }

        /// S8 实测: canBecomeMain=false
        #[method(canBecomeMain)]
        fn can_become_main(&self) -> bool {
            false
        }
    }
);

impl NonActivatingHUDPanel {
    /// 创建 HUD 面板实例 (260px 宽, UI Spec §3.1)
    ///
    /// # Safety
    /// 调用方必须在主线程调用 (AppKit MainThreadOnly 约束)
    #[allow(dead_code)]
    pub unsafe fn new() -> Retained<Self> {
        // SAFETY: 调用方保证主线程 (见文档注释)
        let mtm = MainThreadMarker::new_unchecked();
        let this = mtm.alloc::<Self>().set_ivars(());
        msg_send_id![super(this), init]
    }
}
