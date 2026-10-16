//! Tier 1 自动化增强 — UI Spec §2.4 / §3.1 / §3.2 / Master Spec §4.5
//!
//! 职责 (平台能力缺口补齐, 供主线 HUD / 引导卡接线):
//!   1. 辅助功能权限探测: `is_accessibility_trusted()` (AXIsProcessTrusted)
//!   2. 剪贴板访问权限查询: `pasteboard_access_behavior()` (UI Spec §2.4 accessBehavior,
//!      macOS 15.4+ `NSPasteboard.general.accessBehavior`) + 引导卡触发条件
//!   3. 打开系统隐私设置: `open_pasteboard_privacy_settings()` (Privacy_Pasteboard 面板)
//!   4. ⌘V 按键注入: `simulate_cmd_v()` (Master Spec §4.5 热键表 Tier 1 自动上屏)
//!   5. HUD 坐标定位三级回退: `resolve_hud_position()` 纯逻辑 + 平台查询分离
//!      (UI Spec §3.1: caret → 鼠标 → 活动窗口底部回退)
//!
//! Tier 0 / Tier 1 上屏反馈差异见 UI Spec §3.2: Tier 1 注入 ⌘V 自动上屏,
//! Tier 0 仅写剪贴板并提示「请按 ⌘V」。本模块只提供能力探测与注入原语,
//! 不持有任何 UI 状态; 非 macOS 平台全部安全回退 (不 panic)。

use std::process::Command;

/// HUD 相对文本光标的纵向偏移 (UI Spec §3.1: `caret_rect.maxY + 6px`)
const HUD_CARET_OFFSET_Y: f64 = 6.0;
/// HUD 相对鼠标指针的横向偏移 (UI Spec §3.1 Tier 0: `mouse.x + 12px`)
const HUD_MOUSE_OFFSET_X: f64 = 12.0;
/// HUD 相对鼠标指针的纵向偏移 (UI Spec §3.1 Tier 0: `mouse.y + 12px`)
const HUD_MOUSE_OFFSET_Y: f64 = 12.0;

// ——— 1. 辅助功能权限探测 (Master Spec §4.5 Tier 1 前置条件) ———

/// 探测当前进程是否已授予「辅助功能」权限 (macOS)。
/// 非 macOS 无此概念, 返回 `false` (保守视为 Tier 0)。
/// CI / 未授权环境返回 `false`, 绝不 panic。
pub fn is_accessibility_trusted() -> bool {
    #[cfg(target_os = "macos")]
    {
        // 直接声明 ApplicationServices 框架符号, 不引入额外绑定依赖
        #[link(name = "ApplicationServices", kind = "framework")]
        extern "C" {
            fn AXIsProcessTrusted() -> bool;
        }
        // SAFETY: AXIsProcessTrusted 为无参纯查询 API, 无未定义行为
        unsafe { AXIsProcessTrusted() }
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

// ——— 2. 剪贴板访问权限查询 (UI Spec §2.4 accessBehavior) ———

/// 查询 `NSPasteboard.general.accessBehavior` (macOS 15.4+)。
///
/// 返回值: `"alwaysAllow"` | `"ask"` | `"alwaysDeny"` | `"default"`。
/// - macOS: objc2-app-kit 0.2.2 绑定缺 `accessBehavior` getter,
///   用 `objc2::msg_send!` 直接调 selector, 返回 NSInteger
///   (0=default, 1=ask, 2=alwaysAllow, 3=alwaysDeny, 即 `NSPasteboardAccessBehavior`);
///   selector 不存在 (旧系统) 或调用异常时回退 `"default"`。
/// - 非 macOS: 无此概念, 回退 `"alwaysAllow"`。
pub fn pasteboard_access_behavior() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        use objc2::msg_send;
        use objc2::sel;
        use objc2_app_kit::NSPasteboard;

        // SAFETY: generalPasteboard 返回共享实例; respondsToSelector: 为纯查询;
        // accessBehavior 仅在 respondsToSelector: 通过后才发消息, 避免未定义 selector 崩溃。
        // NSInteger 即 isize, 与 ObjC 消息返回 ABI 一致。
        unsafe {
            let pb = NSPasteboard::generalPasteboard();
            let responds: bool = msg_send![&*pb, respondsToSelector: sel!(accessBehavior)];
            if !responds {
                return "default";
            }
            let raw: isize = msg_send![&*pb, accessBehavior];
            match raw {
                1 => "ask",
                2 => "alwaysAllow",
                3 => "alwaysDeny",
                _ => "default",
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        "alwaysAllow"
    }
}

/// 剪贴板访问权限轻量引导卡触发条件 (UI Spec §2.4):
/// `accessBehavior != .alwaysAllow` 时弹一次性非模态引导。
pub fn needs_pasteboard_access_guide() -> bool {
    pasteboard_access_behavior() != "alwaysAllow"
}

// ——— 3. 打开系统隐私设置 (UI Spec §2.4 主按钮「打开系统隐私设置」) ———

/// 跳转「系统设置 → 隐私与安全性 → 剪贴板」面板 (URL Scheme, UI Spec §2.4)。
/// 失败仅返回 `Err(String)`, 不 panic。
pub fn open_pasteboard_privacy_settings() -> Result<(), String> {
    Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Pasteboard")
        .status()
        .map_err(|e| format!("OPEN_SYSTEM_SETTINGS_FAILED: {e}"))?;
    Ok(())
}

// ——— 4. ⌘V 按键注入 (Master Spec §4.5 Tier 1 自动上屏) ———

/// 自动注入 ⌘V 偏好开关 (设置页 sw-a11y; 默认开启, 权限不足时注入函数自行降级)
static INJECT_ENABLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(true);

pub fn set_inject_enabled(enabled: bool) {
    INJECT_ENABLED.store(enabled, std::sync::atomic::Ordering::Relaxed);
}

pub fn inject_enabled() -> bool {
    INJECT_ENABLED.load(std::sync::atomic::Ordering::Relaxed)
}

/// 是否可执行 Tier 1 自动上屏 (偏好开启 + 辅助功能已授权)
pub fn can_auto_paste() -> bool {
    inject_enabled() && is_accessibility_trusted()
}

/// 注入 ⌘V 完成自动粘贴上屏 (UI Spec §3.2 Tier 1: 写入剪贴板后自动发送 ⌘V)。
///
/// 前置: `is_accessibility_trusted()`, 未授权直接 `Err("ACCESSIBILITY_NOT_GRANTED")`。
/// enigo 0.2 API: `Keyboard::key(Key, Direction) -> InputResult<()>`,
/// 顺序为 `Key::Meta` Press → `Key::Unicode('v')` Click → `Key::Meta` Release。
pub fn simulate_cmd_v() -> Result<(), String> {
    if !is_accessibility_trusted() {
        return Err("ACCESSIBILITY_NOT_GRANTED".to_string());
    }

    use enigo::{Direction, Enigo, Key, Keyboard, Settings};

    let mut kb = Enigo::new(&Settings::default()).map_err(|e| format!("ENIGO_INIT_FAILED: {e}"))?;

    kb.key(Key::Meta, Direction::Press)
        .map_err(|e| format!("ENIGO_KEY_PRESS_FAILED: {e}"))?;

    // 'v' 点击失败也尽力松开 ⌘, 避免修饰键悬挂
    if let Err(e) = kb.key(Key::Unicode('v'), Direction::Click) {
        let _ = kb.key(Key::Meta, Direction::Release);
        return Err(format!("ENIGO_KEY_CLICK_FAILED: {e}"));
    }

    kb.key(Key::Meta, Direction::Release)
        .map_err(|e| format!("ENIGO_KEY_RELEASE_FAILED: {e}"))?;
    Ok(())
}

// ——— 5. HUD 三级定位回退 (UI Spec §3.1) ———

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct CGSize {
    width: f64,
    height: f64,
}

/// 查询文本光标 (caret) 全局坐标, 失败返回 `None`。
///
/// macOS 链路 (UI Spec §3.1 Tier 1): `AXUIElementCreateSystemWide`
/// → `"AXFocusedUIElement"` → 优先 `"AXSelectedTextRange"` (AXValue) 取 origin,
/// 退而求其次取元素 `"AXPosition"` + `"AXSize"` 底边 (x, y + h)。
/// 任何一步失败返回 `None` (安全第一, Result 逐步传播, 不 catch_unwind)。
#[cfg(target_os = "macos")]
pub fn caret_position() -> Option<(f64, f64)> {
    use core_foundation::base::{CFRelease, TCFType};
    use core_foundation::string::CFString;
    use std::ffi::c_void;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXUIElementCreateSystemWide() -> *mut c_void;
        fn AXUIElementCopyAttributeValue(
            elem: *mut c_void,
            attr: *const c_void,
            out: *mut *const c_void,
        ) -> i32; // 0 = kAXErrorSuccess
        fn AXValueGetValue(v: *mut c_void, ty: i32, out: *mut c_void) -> bool;
    }

    /// kAXValueCGPointType = 1 (AXValueType 枚举)
    const K_AX_VALUE_CGPOINT_TYPE: i32 = 1;
    /// kAXValueCGSizeType = 2
    const K_AX_VALUE_CGSIZE_TYPE: i32 = 2;

    /// 读取 AX 元素属性 (调用方负责 CFRelease 返回值)
    fn ax_copy_attr(elem: *const c_void, attr: &CFString) -> Option<*const c_void> {
        let mut out: *const c_void = std::ptr::null();
        // SAFETY: out 由被调方写入; attr 为有效 CFString; 返回 0 时 out 为 +1 持有
        let err = unsafe {
            AXUIElementCopyAttributeValue(
                elem as *mut c_void,
                attr.as_concrete_TypeRef() as *const c_void,
                &mut out,
            )
        };
        if err == 0 && !out.is_null() {
            Some(out)
        } else {
            None
        }
    }

    // SAFETY: 全链路指针有效性与所有权均由 Result/Option 逐步校验, 失败路径逐个 CFRelease
    unsafe {
        let system_wide = AXUIElementCreateSystemWide();
        if system_wide.is_null() {
            return None;
        }

        // systemWide → AXFocusedUIElement
        let focused = ax_copy_attr(system_wide, &CFString::from_static_string("AXFocusedUIElement"));
        CFRelease(system_wide as *const c_void);
        let focused = focused?;

        // 优先: AXSelectedTextRange (AXValue) 取 origin
        let mut result: Option<(f64, f64)> = None;
        if let Some(v) = ax_copy_attr(focused, &CFString::from_static_string("AXSelectedTextRange"))
        {
            let mut pt = CGPoint::default();
            let ok = AXValueGetValue(
                v as *mut c_void,
                K_AX_VALUE_CGPOINT_TYPE,
                &mut pt as *mut CGPoint as *mut c_void,
            );
            CFRelease(v as *const c_void);
            if ok {
                result = Some((pt.x, pt.y));
            }
        }

        // 回退: AXPosition + AXSize 底边 (x, y + height)
        if result.is_none() {
            let pos = ax_copy_attr(focused, &CFString::from_static_string("AXPosition"));
            let size = ax_copy_attr(focused, &CFString::from_static_string("AXSize"));
            if let (Some(pos), Some(size)) = (pos, size) {
                let mut p = CGPoint::default();
                let mut s = CGSize::default();
                let ok_p = AXValueGetValue(
                    pos as *mut c_void,
                    K_AX_VALUE_CGPOINT_TYPE,
                    &mut p as *mut CGPoint as *mut c_void,
                );
                let ok_s = AXValueGetValue(
                    size as *mut c_void,
                    K_AX_VALUE_CGSIZE_TYPE,
                    &mut s as *mut CGSize as *mut c_void,
                );
                if ok_p && ok_s {
                    result = Some((p.x, p.y + s.height));
                }
            }
            if let Some(pos) = pos {
                CFRelease(pos as *const c_void);
            }
            if let Some(size) = size {
                CFRelease(size as *const c_void);
            }
        }

        CFRelease(focused as *const c_void);
        result
    }
}

/// 非 macOS 无 AX 文本光标查询能力, 回退 `None` (UI Spec §3.1 走 Tier 0 鼠标定位)。
#[cfg(not(target_os = "macos"))]
pub fn caret_position() -> Option<(f64, f64)> {
    None
}

/// 查询当前鼠标指针全局坐标 (UI Spec §3.1 Tier 0), 失败返回 `None`。
/// enigo 0.2 API: `Mouse::location(&self) -> InputResult<(i32, i32)>`。
pub fn mouse_position() -> Option<(f64, f64)> {
    use enigo::{Enigo, Mouse, Settings};

    let enigo = Enigo::new(&Settings::default()).ok()?;
    let (x, y) = enigo.location().ok()?;
    Some((f64::from(x), f64::from(y)))
}

/// HUD 坐标三级回退纯函数 (UI Spec §3.1「坐标定位三级回退」):
///   1. caret 有效 → `(caret.x, caret.y + 6.0)` (对齐 `caret_rect.maxY + 6px`)
///   2. 否则鼠标在屏幕内 (0 ≤ x < screen_bounds.0 且 0 ≤ y < screen_bounds.1)
///      → `(mouse.x + 12.0, mouse.y + 12.0)`
///   3. 否则 → `window_fallback` (调用方给「活动窗口底部上 120px」坐标)
///
/// `screen_bounds` 为屏幕尺寸 (width, height), 纯逻辑零副作用, 便于单测。
pub fn resolve_hud_position(
    caret: Option<(f64, f64)>,
    mouse: Option<(f64, f64)>,
    window_fallback: (f64, f64),
    screen_bounds: (f64, f64),
) -> (f64, f64) {
    if let Some(c) = caret {
        return (c.0, c.1 + HUD_CARET_OFFSET_Y);
    }
    if let Some(m) = mouse {
        if m.0 >= 0.0 && m.0 < screen_bounds.0 && m.1 >= 0.0 && m.1 < screen_bounds.1 {
            return (m.0 + HUD_MOUSE_OFFSET_X, m.1 + HUD_MOUSE_OFFSET_Y);
        }
    }
    window_fallback
}

#[cfg(test)]
mod tests {
    use super::*;

    // ——— resolve_hud_position 四级用例 (UI Spec §3.1) ———

    #[test]
    fn hud_prefers_caret_over_mouse() {
        // caret 优先, 鼠标次之
        let pos = resolve_hud_position(
            Some((100.0, 200.0)),
            Some((500.0, 600.0)),
            (10.0, 10.0),
            (1920.0, 1080.0),
        );
        assert_eq!(pos, (100.0, 206.0));
    }

    #[test]
    fn hud_falls_back_to_mouse_when_no_caret() {
        // caret None → 鼠标次之 (+12, +12)
        let pos = resolve_hud_position(None, Some((300.0, 400.0)), (10.0, 10.0), (1920.0, 1080.0));
        assert_eq!(pos, (312.0, 412.0));
    }

    #[test]
    fn hud_falls_back_to_window_when_mouse_out_of_screen() {
        // 鼠标越界 (屏幕外) → window_fallback
        let pos = resolve_hud_position(
            None,
            Some((1920.0, 500.0)),
            (100.0, 900.0),
            (1920.0, 1080.0),
        );
        assert_eq!(pos, (100.0, 900.0));

        // y 越界同样回退
        let pos = resolve_hud_position(
            None,
            Some((100.0, -1.0)),
            (100.0, 900.0),
            (1920.0, 1080.0),
        );
        assert_eq!(pos, (100.0, 900.0));
    }

    #[test]
    fn hud_falls_back_to_window_when_no_caret_and_no_mouse() {
        // caret None + 鼠标 None → window_fallback (活动窗口底部上 120px)
        let pos = resolve_hud_position(None, None, (100.0, 900.0), (1920.0, 1080.0));
        assert_eq!(pos, (100.0, 900.0));
    }

    // ——— 平台能力冒烟测 (不 panic 即可, CI 无权限回退 false/None) ———

    #[test]
    fn accessibility_trusted_returns_bool() {
        let _ = is_accessibility_trusted(); // CI 无权限返回 false, 不 panic
    }

    #[test]
    fn pasteboard_access_behavior_is_legal_value() {
        let behavior = pasteboard_access_behavior();
        assert!(
            ["alwaysAllow", "ask", "alwaysDeny", "default"].contains(&behavior),
            "非法 accessBehavior 返回值: {behavior}"
        );
    }

    #[test]
    fn needs_guide_consistent_with_behavior() {
        // 引导卡触发条件与 behavior 严格一致 (UI Spec §2.4: != .alwaysAllow)
        assert_eq!(
            needs_pasteboard_access_guide(),
            pasteboard_access_behavior() != "alwaysAllow"
        );
    }

    #[test]
    fn caret_and_mouse_position_do_not_panic() {
        // FFI 冒烟: 只要求不 panic、返回 Option
        let _ = caret_position();
        let _ = mouse_position();
    }
}
