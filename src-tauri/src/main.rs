// PasteFlow (灵贴) — v0.1.0 极简菜单栏版入口
// 铁律: v0.1.x 不加载任何 Webview 主面板 (55MB 内存红线)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    pasteflow_lib::run(tauri::generate_context!())
}
