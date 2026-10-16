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
pub mod tier1;
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

/// 本地应用数据存储目录 (~/Library/Application Support/com.pasteflow.app)
pub fn app_data_dir() -> std::path::PathBuf {
    let dir = match std::env::var("HOME") {
        Ok(home) => std::path::PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("com.pasteflow.app"),
        Err(_) => std::path::PathBuf::from(".pasteflow"),
    };
    let _ = std::fs::create_dir_all(&dir);
    dir
}

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
    let data_dir = app_data_dir();
    let db_path = data_dir.join("history.db");
    // P2-8 IPC 状态 (storage + stack + drawer + recipes)
    // P3-2: 历史库与贪吃蛇队列改为共享句柄 (持久化 SQLite WAL, 故障安全回退 in-memory)
    let history_db = match storage::HistoryDb::open(&db_path) {
        Ok(db) => {
            tracing::info!(path = ?db_path, "persistent history database opened");
            std::sync::Arc::new(std::sync::Mutex::new(db))
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to open persistent history database, falling back to in-memory");
            std::sync::Arc::new(std::sync::Mutex::new(
                storage::HistoryDb::in_memory().expect("in-memory history db"),
            ))
        }
    };
    let stack_queue = std::sync::Arc::new(std::sync::Mutex::new(stack::StackQueue::new()));
    let ipc_state = ipc::IpcState::new(
        history_db.clone(),
        stack_queue.clone(),
        recipe::RecipeStore::with_builtin().expect("builtin recipes"),
    );

    let builder = tauri::Builder::default()
        // 缺口1: 全局热键 (TDD §4.5 / Master Spec §4.5 全局登记表, Carbon RegisterEventHotKey)
        .plugin(tauri_plugin_global_shortcut::Builder::new().build());
    let state_for_setup = state.clone();
    // P2-7: 主面板生命周期 + 诊断计数器
    let dashboard_tracker = std::sync::Arc::new(dashboard_window::DashboardTracker::new());
    let diagnostic_counters = std::sync::Arc::new(dashboard_window::DiagnosticCounters::new());
    // P3-1: L3 流式运行时 (BYOK 配置 + 流注册表 + 完成载荷缓存)
    let l3_state = std::sync::Arc::new(l3_ipc::L3IpcState::new());
    if let Some(cfg) = l3_ipc::load_persisted_llm_config(&data_dir) {
        tracing::info!(model = %cfg.model, base = %cfg.base_url, "loaded persisted llm config");
        l3_state.runtime.set_config(cfg);
    }
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
            cmd_tier1_status,
            cmd_set_inject_enabled,
            cmd_open_privacy_settings,
            cmd_get_diagnostic_snapshot,
            cmd_export_sanitized_report,
            l3_ipc::cmd_run_l3_stream,
            l3_ipc::cmd_cancel_l3_stream,
            l3_ipc::cmd_l3_commit_paste,
            l3_ipc::cmd_probe_ollama,
            l3_ipc::cmd_set_llm_config,
            l3_ipc::cmd_get_llm_config,
        ])
        .setup(move |app| {
            use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
            use tauri::tray::TrayIconBuilder;

            // macOS 菜单栏常驻应用形态: 隐藏 Dock 图标, 消除 Cmd+Tab 干扰 (D1 / Master Spec §1.1)
            #[cfg(target_os = "macos")]
            {
                let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }

            // 调试/冒烟启动钩子:
            //   PASTEFLOW_SMOKE=drawer 或 --drawer: 自动展开抽屉
            //   PASTEFLOW_SMOKE=dashboard 或 --dashboard / -d: 自动打开主面板
            #[cfg(debug_assertions)]
            {
                let smoke = std::env::var("PASTEFLOW_SMOKE").unwrap_or_default();
                let is_drawer = smoke.contains("drawer") || std::env::args().any(|a| a == "--drawer");
                let is_dashboard = smoke.contains("dashboard") || std::env::args().any(|a| a == "--dashboard" || a == "-d");
                if is_drawer {
                    let h = app.handle().clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(600));
                        let hc = h.clone();
                        let _ = h.run_on_main_thread(move || drawer_window::toggle(&hc));
                    });
                } else if is_dashboard {
                    let h = app.handle().clone();
                    let tracker = dashboard_tracker.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(600));
                        let hc = h.clone();
                        let _ = h.run_on_main_thread(move || dashboard_window::open(&hc, &tracker));
                    });
                }
            }

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
            let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))
                .unwrap_or_else(|_| tauri::image::Image::from_bytes(include_bytes!("../icons/icon.png")).unwrap());

            TrayIconBuilder::with_id("pasteflow-tray")
                .icon(tray_icon)
                .icon_as_template(true)
                .tooltip("PasteFlow (灵贴)")
                .show_menu_on_left_click(true)
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
            let diag_tick = diagnostic_counters.clone();
            std::thread::spawn(move || {
                let mut last_cc: i64 = current_change_count();
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(25));

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
                    let diag = diag_tick.clone();
                    // 决策在后台 (纯逻辑); 挂载 AppKit 在主线程
                    let outcome = state.evaluate_takeover(&types, volumes, 1);
                    diag.record_change_count(cc);
                    if !outcome.should_take_over {
                        // P3-4: FormatGate 拦截统计 (G1A-G5)
                        diag.record_gate_skip(dashboard_window::gate_id(outcome.verdict));
                        continue;
                    }
                    if state.clipboard_owner.is_internal_write(cc)
                        || state.loop_guard.is_internal_write(cc)
                    {
                        continue;
                    }
                    let handle_emit = handle.clone();
                    let diag_inner = diag.clone();
                    let _ = handle.run_on_main_thread(move || {
                        let front = paste_provider::frontmost_bundle();
                        let source_text = clipboard_plain_text();
                        // P3-2: 复制事件 → 历史入库 + 贪吃蛇入队 (抽屉数据源)
                        let recorded = ipc::record_copy_entry(&history, &stack, &source_text, &front);
                        if recorded {
                            let ev = serde_json::json!({ "active": stack.lock().unwrap().is_active() });
                            let _ = handle_emit.emit_to("drawer", "pasteflow://stack-updated", ev);
                            let _ = handle_emit.emit_to("drawer", "pasteflow://drawer-refresh", ());
                            let _ = handle_emit.emit("pasteflow://history-updated", ());
                        }
                        // 断链修复: Toast「已复制」+ 双击 ⌘C⌘C 唤醒 Micro-HUD
                        // (tick 已判定非内部写入; 此处仅消抖/双击判定, 不走 allow_change 的 3s 冷却)
                        let snapshot_hash = {
                            use std::hash::{Hash, Hasher};
                            let mut h = std::collections::hash_map::DefaultHasher::new();
                            source_text.hash(&mut h);
                            h.finish()
                        };
                        match state.double_copy.on_copy(snapshot_hash, true) {
                            crate::double_copy::CopyEventKind::Debounced => {}
                            crate::double_copy::CopyEventKind::Normal => {
                                state.show_toast(crate::toast::ToastRequest {
                                    text: "已复制".into(),
                                    level: crate::toast::ToastLevel::Info,
                                });
                            }
                            crate::double_copy::CopyEventKind::DoubleClick => {
                                tracing::info!(
                                    snapshot_hash = snapshot_hash,
                                    front = %front,
                                    source_len = source_text.len(),
                                    "double-copy detected -> summoning micro-hud"
                                );
                                let payload = crate::hud::summon_on_double_copy(
                                    snapshot_hash,
                                    &front,
                                    &source_text,
                                );
                                state.hud_summon.emit(payload.clone());
                                hud_window::broadcast_summon(&handle_emit, &payload);
                                state.show_toast(crate::toast::ToastRequest {
                                    text: "已唤醒 Micro-HUD (按 1–4 选择动作)".into(),
                                    level: crate::toast::ToastLevel::Info,
                                });
                            }
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
                            diagnostics: diag_inner.clone(),
                        };
                        if let Some(new_cc) = paste_provider::mount_provider(Arc::new(deps)) {
                            tracing::info!(change_count = new_cc, "provider mounted (takeover)");
                        }
                    });
                }
            });

            // P2-1: 2s 后预热常驻隐藏 micro-hud (IPC Spec §1)
            hud_window::preheat(app.handle().clone());

            // 缺口1: 注册 5 个全局激活热键 (TDD §4.5 / Master Spec §4.5)
            {
                use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
                let hs = app.global_shortcut();
                for hk in hotkeys::ACTIVATION_HOTKEYS {
                    let id = hk.id.to_string();
                    let state_hk = state_for_setup.clone();
                    if let Err(e) = hs.on_shortcut(hk.plugin, move |app, _sc, ev| {
                        if ev.state != ShortcutState::Pressed {
                            return;
                        }
                        handle_global_hotkey(app, &id, &state_hk);
                    }) {
                        tracing::warn!(hotkey = hk.id, error = %e, "global shortcut register failed");
                    }
                }
            }

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

// —— 缺口1: 全局热键动作分发 (TDD §4.5 / Master Spec §4.5 全局登记表) ——

/// 5 个激活热键的动作分发（快捷键注册回调线程; AppKit 操作经 run_on_main_thread 串行）
fn handle_global_hotkey(app: &tauri::AppHandle, id: &str, state: &Arc<EngineState>) {
    use tauri::{Emitter, Manager};
    match id {
        // ⌘⇧P: 自动挡暂停/恢复 (暂停前自动临终物化, R4)
        "autopilot_pause" => {
            let st = state.clone();
            let _ = app.clone().run_on_main_thread(move || {
                let mut native = engine::setup_native(&st);
                st.handle_menu_action(MenuAction::ToggleEnabled, &mut native);
            });
        }
        // ⌃⌥V: 手动挡备选热键 -> 唤醒 Micro-HUD (UIDoc: v0.1.0 还原原文 / v0.1.5 语义升级)
        "manual_summon" => {
            let source_text = clipboard_plain_text();
            let front = paste_provider::frontmost_bundle();
            let payload = hud::summon_on_hotkey(0, &front, &source_text);
            state.hud_summon.emit(payload.clone());
            let handle = app.clone();
            let _ = app.clone().run_on_main_thread(move || {
                hud_window::broadcast_summon(&handle, &payload);
            });
        }
        // ⌃⌥D: 显式 DLP 脱敏粘贴 (密钥 -> <REDACTED>, UI Spec §2.3 #5)
        "dlp_redact" => {
            let st = state.clone();
            let handle = app.clone();
            let _ = app.clone().run_on_main_thread(move || {
                let src = clipboard_plain_text();
                if src.is_empty() {
                    return;
                }
                let (redacted, hits) = st.dlp.redact(&src);
                if hits == 0 {
                    return; // 无密钥不改写 (D8: 静默三不, 不惊扰)
                }
                l3_ipc::write_plain_to_pasteboard(&redacted, st.clipboard_owner.clone());
                st.show_toast(toast::ToastRequest {
                    text: format!("🛡️ 已将 {hits} 处敏感密钥替换为 <REDACTED>"),
                    level: toast::ToastLevel::SilentDlp,
                });
                let _ = handle.emit("pasteflow://history-updated", ());
            });
        }
        // ⌥⇧V: 历史与贪吃蛇抽屉 (磁吸侧边抽屉)
        "drawer_history" => {
            let handle = app.clone();
            let _ = app.clone().run_on_main_thread(move || drawer_window::toggle(&handle));
        }
        // ⌥⇧S: 贪吃蛇模式开关 (关闭时队列清空, IPC Spec §3.2)
        "stack_mode" => {
            let ipc = app.state::<ipc::IpcState>();
            let mut stack = ipc.stack.lock().unwrap();
            let next = !stack.is_active();
            ipc::set_stack_mode(&mut stack, next);
            let ev = serde_json::json!({ "active": next });
            let handle = app.clone();
            let _ = app.clone().run_on_main_thread(move || {
                let _ = handle.emit_to("drawer", "pasteflow://stack-updated", ev.clone());
                let _ = handle.emit("pasteflow://history-updated", ());
            });
        }
        other => tracing::warn!(id = other, "unknown global hotkey id"),
    }
}

// —— P2-1 HUD 命令 (IPC Spec §3) ——

#[derive(serde::Deserialize)]
struct HudVisibleArgs {
    visible: bool,
}

#[tauri::command]
fn cmd_hud_visible(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<engine::EngineState>>,
    args: HudVisibleArgs,
) {
    state.hud_visibility.set_visible(args.visible);
    if !args.visible {
        hud_window::hide(&app);
    }
}

#[derive(serde::Deserialize)]
struct HudTriggerArgs {
    #[serde(default, rename = "slotKey")]
    slot_key: Option<String>,
    #[serde(default, rename = "slotKeys")]
    slot_keys: Option<Vec<String>>,
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
    let slot = args
        .slot_key
        .or_else(|| args.slot_keys.and_then(|keys| keys.into_iter().next()))
        .unwrap_or_default();
    match slot.as_str() {
        // 槽位 0: 还原原文 (builtin.restore-raw) — R4 主线程物化原始文本
        "0" | "Backspace" => {
            use crate::materializer::MaterializeReason;
            let engine = state.inner();
            let mut native = engine::setup_native(engine);
            engine.lifecycle_materialize(MaterializeReason::RestoreRaw, &mut native);
        }
        // 槽位 1–4: 执行滤镜变换写剪贴板 + Tier 1 自动注入 ⌘V (UI Spec §3.2)
        "1" | "2" | "3" | "4" => {
            execute_hud_slot(&app, &state, &slot);
        }
        _ => {}
    }
}

/// 槽位动作执行: 推荐动作查 pipeId → L1 变换 → 原子写剪贴板 → (Tier 1) 注入 ⌘V
fn execute_hud_slot(app: &tauri::AppHandle, state: &Arc<EngineState>, slot_key: &str) {
    use tauri::Emitter;
    let front = paste_provider::frontmost_bundle();
    let Some(action) = hud::recommend_actions(&front)
        .into_iter()
        .find(|a| a.slot_key == slot_key)
    else {
        return;
    };
    let source = clipboard_plain_text();
    // L1 变换 (纯函数, L3 槽位由前端分流 startL3 不进此路径)
    let transformed: Option<String> = match action.pipe_id.as_str() {
        "builtin.markdown-to-im-cleaned" => Some(filters::markdown_to_im_text(&source)),
        "builtin.markdown-to-rich" => Some(filters::markdown_to_html(&source)),
        "builtin.markdown-to-code" | "builtin.restore-raw" => Some(source.clone()),
        "builtin.social-link-clean" => Some(filters::clean_social_text(&source).text),
        _ => return, // L3 槽位 / 未知 pipeId 不在此处理
    };
    let Some(text) = transformed else { return };
    l3_ipc::write_plain_to_pasteboard(&text, state.clipboard_owner.clone());
    // Tier 1: 授权 + 偏好开启 → 自动注入 ⌘V; 否则 Tier 0 由 HUD 提示「请按 ⌘V」
    if tier1::can_auto_paste() {
        if let Err(e) = tier1::simulate_cmd_v() {
            tracing::warn!(error = %e, "tier1 cmd-v injection failed");
        }
    }
    let _ = app.emit("pasteflow://history-updated", ());
}

// —— 缺口5: Tier 1 权限状态与引导 (UI Spec §2.4 accessBehavior 引导卡) ——

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Tier1StatusDto {
    accessibility_trusted: bool,
    pasteboard_access_behavior: &'static str,
    needs_guide: bool,
    inject_enabled: bool,
}

#[tauri::command]
fn cmd_tier1_status() -> Tier1StatusDto {
    Tier1StatusDto {
        accessibility_trusted: tier1::is_accessibility_trusted(),
        pasteboard_access_behavior: tier1::pasteboard_access_behavior(),
        needs_guide: tier1::needs_pasteboard_access_guide(),
        inject_enabled: tier1::inject_enabled(),
    }
}

#[derive(serde::Deserialize)]
struct SetInjectArgs {
    enabled: bool,
}

#[tauri::command]
fn cmd_set_inject_enabled(args: SetInjectArgs) -> Tier1StatusDto {
    tier1::set_inject_enabled(args.enabled);
    cmd_tier1_status()
}

#[tauri::command]
fn cmd_open_privacy_settings() -> Result<(), String> {
    tier1::open_pasteboard_privacy_settings()
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
