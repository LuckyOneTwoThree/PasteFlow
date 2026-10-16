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
pub mod recipe_import;
pub mod ipc;
pub mod hud_window;
pub mod drawer_window;
pub mod dashboard_window;
pub mod l3;
pub mod l3_ipc;
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
    use tauri::Emitter;
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pasteflow_lib=info".into()),
        )
        .init();
    tracing::info!("PasteFlow engine starting (v0.1.0, zero-webview)");

    let state = Arc::new(EngineState::new());
    // P2-8 IPC 状态 (storage + stack + drawer + recipes)
    // P3-2: 历史库与贪吃蛇队列改为共享句柄 (引擎复制事件写入, 抽屉读取同一份)
    let history_db = std::sync::Arc::new(std::sync::Mutex::new(
        storage::HistoryDb::in_memory().expect("in-memory history db"),
    ));
    let stack_queue = std::sync::Arc::new(std::sync::Mutex::new(stack::StackQueue::new()));
    let ipc_state = ipc::IpcState::new(
        history_db.clone(),
        stack_queue.clone(),
        recipe::RecipeStore::with_builtin().expect("builtin recipes"),
    );

    let builder = tauri::Builder::default();
    let state_for_setup = state.clone();
    // P2-7: 主面板生命周期 + 诊断计数器
    let dashboard_tracker = std::sync::Arc::new(dashboard_window::DashboardTracker::new());
    let diagnostic_counters = std::sync::Arc::new(dashboard_window::DiagnosticCounters::new());
    // P3-1: L3 流式运行时 (BYOK 配置 + 流注册表 + 完成载荷缓存)
    let l3_state = std::sync::Arc::new(l3_ipc::L3IpcState::new());
    builder
        .manage(ipc_state)
        .manage(state.clone())
        .manage(dashboard_tracker.clone())
        .manage(diagnostic_counters.clone())
        .manage(l3_state)
        .invoke_handler(tauri::generate_handler![
            ipc::cmd_search_history,
            ipc::cmd_toggle_pin_item,
            ipc::cmd_set_stack_mode,
            ipc::cmd_consume_stack_queue,
            ipc::cmd_list_stack,
            ipc::cmd_remove_stack_item,
            ipc::cmd_reorder_stack_items,
            ipc::cmd_import_pipe_recipe,
            ipc::cmd_list_pipe_recipes,
            drawer_window::cmd_drawer_visible,
            cmd_hud_visible,
            cmd_hud_trigger,
            cmd_get_diagnostic_snapshot,
            cmd_export_sanitized_report,
            l3_ipc::cmd_run_l3_stream,
            l3_ipc::cmd_cancel_l3_stream,
            l3_ipc::cmd_l3_commit_paste,
            l3_ipc::cmd_probe_ollama,
            l3_ipc::cmd_set_llm_config,
        ])
        .setup(move |app| {
            use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
            use tauri::tray::TrayIconBuilder;

            let enable = MenuItem::with_id(app, "enable", "启用 PasteFlow", true, None::<&str>)?;
            let sep1 = PredefinedMenuItem::separator(app)?;
            let md = MenuItem::with_id(app, "filter-md", "Markdown → 富文本", true, None::<&str>)?;
            let social = MenuItem::with_id(app, "filter-social", "社媒链接净化", true, None::<&str>)?;
            let sep2 = PredefinedMenuItem::separator(app)?;
            let history = MenuItem::with_id(app, "history", "打开剪贴板历史", true, None::<&str>)?;
            let diag = MenuItem::with_id(app, "diagnostics", "诊断看板", true, None::<&str>)?;
            let sep3 = PredefinedMenuItem::separator(app)?;
            let quit = MenuItem::with_id(app, "quit", "退出 PasteFlow", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&enable, &sep1, &md, &social, &sep2, &history, &diag, &sep3, &quit])?;

            let handle = app.handle().clone();
            let state_for_menu = state_for_setup.clone();
            TrayIconBuilder::with_id("pasteflow-tray")
                .tooltip("PasteFlow (灵贴)")
                .menu(&menu)
                .on_menu_event(move |app, event| {
                    // P3-2: 「剪贴板历史」= 磁吸抽屉 (⌥⇧V 同义)；「诊断看板」= 主面板
                    if event.id().as_ref() == "history" {
                        let handle = app.clone();
                        let _ = handle.clone().run_on_main_thread(move || {
                            drawer_window::toggle(&handle);
                        });
                        return;
                    }
                    if event.id().as_ref() == "diagnostics" {
                        let tracker = dashboard_tracker.clone();
                        let handle = app.clone();
                        let _ = handle.clone().run_on_main_thread(move || {
                            dashboard_window::open(&handle, &tracker);
                        });
                        return;
                    }
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
            let history_tick = history_db.clone();
            let stack_tick = stack_queue.clone();
            std::thread::spawn(move || {
                let mut last_cc: i64 = current_change_count();
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(50));

                    // P2-1: HUD 唤醒总线 drain (双击 ⌘C⌘C -> Re-own + 唤醒 Micro-HUD)
                    let summons = state_tick.hud_summon.drain();
                    if !summons.is_empty() {
                        let handle_summon = handle_tick.clone();
                        let _ = handle_tick.run_on_main_thread(move || {
                            for payload in &summons {
                                hud_window::broadcast_summon(&handle_summon, payload);
                            }
                        });
                    }

                    let cc = current_change_count();
                    if cc == last_cc {
                        continue;
                    }
                    last_cc = cc;
                    let types = clipboard_types();
                    let volumes = clipboard_volumes();
                    let state = state_tick.clone();
                    let handle = handle_tick.clone();
                    let history = history_tick.clone();
                    let stack = stack_tick.clone();
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
                    let handle_emit = handle.clone();
                    let _ = handle.run_on_main_thread(move || {
                        let front = paste_provider::frontmost_bundle();
                        let source_text = clipboard_plain_text();
                        // P3-2: 复制事件 → 历史入库 + 贪吃蛇入队 (抽屉数据源)
                        let recorded = ipc::record_copy_entry(&history, &stack, &source_text, &front);
                        if recorded {
                            let ev = serde_json::json!({ "active": stack.lock().unwrap().is_active() });
                            let _ = handle_emit.emit_to("drawer", "pasteflow://stack-updated", ev);
                        }
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

            // P2-1: 2s 后预热常驻隐藏 micro-hud (IPC Spec §1)
            hud_window::preheat(app.handle().clone());

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

// —— P2-1 HUD 命令 (IPC Spec §3) ——

#[derive(serde::Deserialize)]
struct HudVisibleArgs {
    visible: bool,
}

#[tauri::command]
fn cmd_hud_visible(state: tauri::State<'_, Arc<engine::EngineState>>, args: HudVisibleArgs) {
    state.hud_visibility.set_visible(args.visible);
}

#[derive(serde::Deserialize)]
struct HudTriggerArgs {
    #[serde(rename = "slotKey")]
    slot_key: String,
}

/// HUD 槽位动作触发 (UI Spec §3.2: 1–4 选中 / 0·Backspace 还原原文)
#[tauri::command]
fn cmd_hud_trigger(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<engine::EngineState>>,
    args: HudTriggerArgs,
) {
    hud_window::hide(&app);
    state.hud_visibility.set_visible(false);
    match args.slot_key.as_str() {
        // 槽位 0: 还原原文 (builtin.restore-raw) — R4 主线程物化原始文本
        "0" | "Backspace" => {
            use crate::materializer::MaterializeReason;
            let engine = state.inner();
            let mut native = engine::setup_native(engine);
            engine.lifecycle_materialize(MaterializeReason::RestoreRaw, &mut native);
        }
        // 槽位 1–4: 动作上屏 (Tier 0 自动挡已接管, 由消费方 ⌘V)
        "1" | "2" | "3" | "4" => {
            tracing::info!(slot = %args.slot_key, "hud slot triggered (tier0 ready to paste)");
        }
        _ => {}
    }
}

// —— P2-7 诊断看板命令 (IPC Spec §3) ——

#[tauri::command]
fn cmd_get_diagnostic_snapshot(
    counters: tauri::State<'_, std::sync::Arc<dashboard_window::DiagnosticCounters>>,
) -> dashboard_window::DiagnosticSnapshotDto {
    counters.snapshot()
}

#[derive(serde::Deserialize)]
struct ExportReportArgs {
    #[serde(rename = "targetPath")]
    target_path: String,
}

#[tauri::command]
fn cmd_export_sanitized_report(
    counters: tauri::State<'_, std::sync::Arc<dashboard_window::DiagnosticCounters>>,
    ipc: tauri::State<'_, ipc::IpcState>,
    args: ExportReportArgs,
) -> Result<serde_json::Value, String> {
    let db = ipc.history.lock().unwrap();
    let path = dashboard_window::export_sanitized_report(&counters, Some(&db), &args.target_path)?;
    Ok(serde_json::json!({ "savedPath": path }))
}
