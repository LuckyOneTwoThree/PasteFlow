// PasteFlow (灵贴) Rust 核心引擎 — TDD v1.0 §1.1 模块拓扑
//
// 依赖方向: clipboard_owner / materializer -> pipe_executor -> {filters} + hotkeys
//           window_observer / format_gate / privacy_guard / double_copy 为独立模块
pub mod window_observer;
pub mod format_gate;
pub mod privacy_guard;
pub mod filters;
pub mod pipe_executor;
pub mod clipboard_owner;
pub mod double_copy;
pub mod materializer;
pub mod hotkeys;
pub mod toast;
pub mod menu_bar;
pub mod native_bridge;
pub mod recipe;
pub mod engine;
pub mod paste_provider;
pub mod storage;
pub mod hud;
pub mod hud_state;
pub mod drawer;
pub mod stack;
#[cfg(target_os = "macos")]
pub mod hud_panel;

use engine::EngineState;
use menu_bar::MenuAction;
use std::sync::Arc;

/// v0.1.0 入口: 零 Webview 极简菜单栏版 (Dev Plan P1-15c)
///
/// 装配:
///   1. TrayIcon + Menu (控制中心, 对齐 IPC Spec §3.1)
///   2. 50ms changeCount 轮询 -> evaluate_takeover -> run_on_main_thread 挂载 Provider
///   3. 退出/暂停 -> Materializer 四步铁律物化 (R4)
///
/// 铁律: 不创建任何 tauri Window (55MB 内存红线, 零 Webview)
pub fn run(context: tauri::Context<tauri::Wry>) {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pasteflow_lib=info".into()),
        )
        .init();
    tracing::info!("PasteFlow engine starting (v0.1.0, zero-webview)");

    let state = Arc::new(EngineState::new());

    let builder = tauri::Builder::default();
    let state_for_setup = state.clone();
    builder
        .setup(move |app| {
            use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
            use tauri::tray::TrayIconBuilder;

            let enable = MenuItem::with_id(app, "enable", "启用 PasteFlow", true, None::<&str>)?;
            let sep1 = PredefinedMenuItem::separator(app)?;
            let md = MenuItem::with_id(app, "filter-md", "Markdown → 富文本", true, None::<&str>)?;
            let social = MenuItem::with_id(app, "filter-social", "社媒链接净化", true, None::<&str>)?;
            let sep2 = PredefinedMenuItem::separator(app)?;
            let quit = MenuItem::with_id(app, "quit", "退出 PasteFlow", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&enable, &sep1, &md, &social, &sep2, &quit])?;

            let handle = app.handle().clone();
            let state_for_menu = state_for_setup.clone();
            TrayIconBuilder::with_id("pasteflow-tray")
                .tooltip("PasteFlow (灵贴)")
                .menu(&menu)
                .on_menu_event(move |app, event| {
                    let action = match event.id().as_ref() {
                        "enable" => Some(MenuAction::ToggleEnabled),
                        "filter-md" => Some(MenuAction::ToggleFilter(
                            menu_bar::FilterId::Markdown,
                        )),
                        "filter-social" => Some(MenuAction::ToggleFilter(
                            menu_bar::FilterId::SocialLink,
                        )),
                        "quit" => Some(MenuAction::Quit),
                        _ => None,
                    };
                    if let Some(action) = action {
                        let state = state_for_menu.clone();
                        let handle = app.clone();
                        // R4: 物化必须在主线程; 退出走 app.exit
                        let _ = handle.clone().run_on_main_thread(move || {
                            let mut native = engine::setup_native(&state);
                            state.handle_menu_action(action, &mut native);
                            if matches!(action, MenuAction::Quit) {
                                handle.exit(0);
                            }
                        });
                    }
                })
                .build(app)?;

            // 50ms changeCount 轮询 (后台线程); 决策后挂载在主线程执行
            let state_tick = state_for_setup.clone();
            let handle_tick = handle.clone();
            std::thread::spawn(move || {
                let mut last_cc: i64 = current_change_count();
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    let cc = current_change_count();
                    if cc == last_cc {
                        continue;
                    }
                    last_cc = cc;
                    let types = clipboard_types();
                    let volumes = clipboard_volumes();
                    let state = state_tick.clone();
                    let handle = handle_tick.clone();
                    // 决策在后台 (纯逻辑); 挂载 AppKit 在主线程
                    let outcome = state.evaluate_takeover(&types, volumes, 1);
                    if !outcome.should_take_over {
                        continue;
                    }
                    if state.clipboard_owner.is_internal_write(cc)
                        || state.loop_guard.is_internal_write(cc)
                    {
                        continue;
                    }
                    let _ = handle.run_on_main_thread(move || {
                        let front = paste_provider::frontmost_bundle();
                        let source_text = clipboard_plain_text();
                        let deps = paste_provider::ProviderDeps {
                            window_observer: state.window_observer.clone(),
                            pipe_executor: state.pipe_executor.clone(),
                            clipboard_owner: state.clipboard_owner.clone(),
                            double_copy: state.double_copy.clone(),
                            dlp: state.dlp.clone(),
                            mounted_at: std::time::Instant::now(),
                            source_bundle: front,
                            generation: 0,
                            source_text,
                        };
                        if let Some(new_cc) = paste_provider::mount_provider(Arc::new(deps)) {
                            tracing::info!(change_count = new_cc, "provider mounted (takeover)");
                        }
                    });
                }
            });

            Ok(())
        })
        .run(context)
        .expect("PasteFlow app failed to run");
}

/// 当前剪贴板 changeCount (objc 胶水, 仅主线程安全读取; 后台只读原子快照)
#[cfg(target_os = "macos")]
fn current_change_count() -> i64 {
    unsafe {
        let pb = objc2_app_kit::NSPasteboard::generalPasteboard();
        pb.changeCount() as i64
    }
}

#[cfg(not(target_os = "macos"))]
fn current_change_count() -> i64 {
    0
}

/// 剪贴板类型列表 (UTI 字符串)
#[cfg(target_os = "macos")]
fn clipboard_types() -> Vec<String> {
    use objc2_app_kit::NSPasteboard;
    unsafe {
        let pb = NSPasteboard::generalPasteboard();
        match pb.types() {
            Some(arr) => {
                let mut out = Vec::with_capacity(arr.count() as usize);
                for i in 0..arr.count() as usize {
                    if let Some(t) = arr.get(i) {
                        out.push(t.to_string());
                    }
                }
                out
            }
            None => Vec::new(),
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn clipboard_types() -> Vec<String> {
    Vec::new()
}

/// 剪贴板体积分级输入 (G5)
#[cfg(target_os = "macos")]
fn clipboard_volumes() -> crate::format_gate::VolumeBytes {
    use crate::format_gate::VolumeBytes;
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeHTML, NSPasteboardTypeRTF, NSPasteboardTypeString};
    unsafe {
        let pb = NSPasteboard::generalPasteboard();
        fn sz(pb: &NSPasteboard, t: &objc2_app_kit::NSPasteboardType) -> usize {
            unsafe { pb.dataForType(t) }.map(|d| d.len() as usize).unwrap_or(0)
        }
        VolumeBytes {
            html: sz(&pb, NSPasteboardTypeHTML),
            rtf: sz(&pb, NSPasteboardTypeRTF),
            plain: sz(&pb, NSPasteboardTypeString),
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn clipboard_volumes() -> crate::format_gate::VolumeBytes {
    crate::format_gate::VolumeBytes { html: 0, rtf: 0, plain: 0 }
}

/// 剪贴板纯文本 (source 缓存, 供 Provider 路由)
#[cfg(target_os = "macos")]
fn clipboard_plain_text() -> String {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
    unsafe {
        NSPasteboard::generalPasteboard()
            .stringForType(NSPasteboardTypeString)
            .map(|s| s.to_string())
            .unwrap_or_default()
    }
}

#[cfg(not(target_os = "macos"))]
fn clipboard_plain_text() -> String {
    String::new()
}
