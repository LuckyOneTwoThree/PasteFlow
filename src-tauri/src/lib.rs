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

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pasteflow_lib=info".into()),
        )
        .init();
    tracing::info!("PasteFlow engine starting (v0.1.0)");
}
