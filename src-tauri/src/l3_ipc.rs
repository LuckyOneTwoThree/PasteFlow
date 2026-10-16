//! P3-1 收尾: L3 流式 IPC 命令层 (IPC Spec v1.0 §3.2 / §4)
//!
//! - `cmd_run_l3_stream`：启动后台 SSE 流，逐块 emit `pasteflow://l3-stream-chunk`
//! - `cmd_cancel_l3_stream`：Esc 即时熔断（`StreamRegistry` 原子标志）
//! - `cmd_l3_commit_paste`：生成完毕一次性原子粘贴（R-5 防撤销栈碎裂）
//! - `cmd_probe_ollama` / `cmd_set_llm_config`：主面板 Tab 3 设置
//!
//! DLP 铁律（Master Spec §8.3）：命中密钥的文本禁止送云端 → `L3_DLP_BLOCKED`。

use crate::l3::{
    run_l3, L3Error, L3Recipes, L3Runtime, L3Session, LlmConfig, PromptType,
};
use crate::privacy_guard::{DlpAction, DlpDetector};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::Emitter;

/// L3 IPC 状态（已完成载荷缓存，供 `cmd_l3_commit_paste` 一次性上屏）
pub struct L3IpcState {
    pub runtime: L3Runtime,
    completed: Mutex<HashMap<String, crate::l3::AtomicPastePayload>>,
}

impl L3IpcState {
    pub fn new() -> Self {
        Self {
            runtime: L3Runtime::new(),
            completed: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for L3IpcState {
    fn default() -> Self {
        Self::new()
    }
}

// —— IPC DTO (IPC Spec §3.2 / §4) ——

#[derive(Debug, Serialize)]
pub struct RunL3Return {
    #[serde(rename = "streamId")]
    pub stream_id: String,
}

#[derive(Debug, Serialize)]
pub struct CancelL3Return {
    pub cancelled: bool,
}

#[derive(Debug, Serialize)]
pub struct CommitPasteReturn {
    /// Tier 0: 写回剪贴板等待用户 ⌘V (UI Spec §3.2)
    pub mode: &'static str,
}

#[derive(Debug, Serialize)]
pub struct SetLlmConfigReturn {
    pub saved: bool,
}

/// `pasteflow://l3-stream-chunk` 事件载荷 (IPC Spec §4)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct L3ChunkEvent {
    pub stream_id: String,
    pub delta: String,
    pub accumulated_text: String,
    pub done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_token_latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

// —— 核心逻辑（自由函数，单测覆盖） ——

/// 前缀 + 用户内容 → L3 提示词（安全前缀只读锁定）
pub fn build_user_content(prompt: &str, source_text: &str) -> String {
    format!("{prompt}\n\n---\n{source_text}")
}

/// DLP 防护判定：命中密钥 → 禁止送云端
pub fn dlp_blocks(source_text: &str) -> bool {
    DlpDetector::new().detect(source_text) == DlpAction::SilentGuard
}

/// 生成 streamId（uuid v7，单调递增可排序）
pub fn new_stream_id() -> String {
    format!("l3-{}", uuid::Uuid::now_v7())
}

// —— Tauri Commands ——

/// 启动 L3 流式（核心逻辑, 供 `cmd_run_l3_stream` 与 `cmd_consume_stack_queue(L3_AI_SUMMARY)` 共用）
///
/// 后台线程执行流式请求（ureq 阻塞 IO 不占 async 运行时），逐块
/// emit `pasteflow://l3-stream-chunk` 到 micro-hud / drawer。
pub fn spawn_l3_stream(
    app: &tauri::AppHandle,
    l3: &std::sync::Arc<L3IpcState>,
    prompt_type: PromptType,
    source_text: &str,
) -> Result<String, crate::ipc::IpcError> {
    let cfg = l3.runtime.config().ok_or_else(|| {
        crate::ipc::IpcError::new("L3_CONFIG_MISSING", "请先在设置中配置 LLM (BYOK / Ollama)")
    })?;
    cfg.validate().map_err(|e| match e {
        L3Error::ConfigMissing => crate::ipc::IpcError::new("L3_CONFIG_MISSING", "LLM 配置不完整"),
        other => crate::ipc::IpcError::new(other.code(), other.code()),
    })?;
    if dlp_blocks(source_text) {
        return Err(crate::ipc::IpcError::new(
            "L3_DLP_BLOCKED",
            "检测到敏感凭证，已禁止送入云端模型 (D8)",
        ));
    }
    let recipes = L3Recipes::with_builtin();
    let system_prompt = recipes
        .prompt(prompt_type)
        .ok_or_else(|| crate::ipc::IpcError::new("L3_CONFIG_MISSING", "未知提示词类型"))?
        .to_string();

    let stream_id = new_stream_id();
    let cancel_flag = l3.runtime.registry.register(&stream_id);

    let l3_state = l3.clone();
    let app_bg = app.clone();
    let sid = stream_id.clone();
    let user_content = build_user_content(&system_prompt, source_text);
    std::thread::spawn(move || {
        let provider = crate::l3::HttpProvider::new(cfg);
        let mut session = L3Session::new(&system_prompt);
        let mut first_latency: Option<u64> = None;
        let sid_chunk = sid.clone();
        let app_chunk = app_bg.clone();
        let flag = cancel_flag.clone();
        let result = {
            let mut cb = |idx: usize, delta: &str, acc: &str| -> bool {
                if flag.load(std::sync::atomic::Ordering::SeqCst) {
                    return false;
                }
                let ev = L3ChunkEvent {
                    stream_id: sid_chunk.clone(),
                    delta: delta.to_string(),
                    accumulated_text: acc.to_string(),
                    done: false,
                    first_token_latency_ms: if idx == 0 { Some(0) } else { None },
                    error_code: None,
                };
                let _ = app_chunk.emit_to("micro-hud", "pasteflow://l3-stream-chunk", &ev);
                let _ = app_chunk.emit_to("drawer", "pasteflow://l3-stream-chunk", &ev);
                if idx == 0 {
                    first_latency = Some(0);
                }
                true
            };
            run_l3(
                &provider,
                &provider.config,
                &system_prompt,
                &user_content,
                false,
                &mut session,
                Some(&mut cb),
            )
        };
        let ev = match &result {
            Ok(payload) => {
                l3_state
                    .completed
                    .lock()
                    .unwrap()
                    .insert(sid.clone(), payload.clone());
                L3ChunkEvent {
                    stream_id: sid.clone(),
                    delta: String::new(),
                    accumulated_text: payload.plain_text.clone(),
                    done: true,
                    first_token_latency_ms: first_latency.or(Some(payload.first_chunk_ms)),
                    error_code: None,
                }
            }
            Err(e) => L3ChunkEvent {
                stream_id: sid.clone(),
                delta: String::new(),
                accumulated_text: session.preview().to_string(),
                done: true,
                first_token_latency_ms: first_latency,
                error_code: Some(e.code().to_string()),
            },
        };
        let _ = app_bg.emit_to("micro-hud", "pasteflow://l3-stream-chunk", &ev);
        let _ = app_bg.emit_to("drawer", "pasteflow://l3-stream-chunk", &ev);
        l3_state.runtime.registry.finish(&sid);
    });

    Ok(stream_id)
}

/// 启动 L3 流式请求（IPC Spec §3.2）
#[tauri::command]
pub async fn cmd_run_l3_stream(
    app: tauri::AppHandle,
    l3: tauri::State<'_, std::sync::Arc<L3IpcState>>,
    prompt_type: PromptType,
    source_text: String,
) -> Result<RunL3Return, crate::ipc::IpcError> {
    let stream_id = spawn_l3_stream(&app, l3.inner(), prompt_type, &source_text)?;
    Ok(RunL3Return { stream_id })
}

/// Esc / `cmd_cancel_l3_stream`：即时熔断 HTTP 流 (R-5)
#[tauri::command]
pub fn cmd_cancel_l3_stream(
    l3: tauri::State<'_, std::sync::Arc<L3IpcState>>,
    stream_id: String,
) -> CancelL3Return {
    CancelL3Return {
        cancelled: l3.runtime.registry.cancel(&stream_id),
    }
}

/// 生成完毕 → 一次性原子粘贴（写回剪贴板，Tier 0 等待 ⌘V）
#[tauri::command]
pub fn cmd_l3_commit_paste(
    engine: tauri::State<'_, std::sync::Arc<crate::engine::EngineState>>,
    l3: tauri::State<'_, std::sync::Arc<L3IpcState>>,
    stream_id: String,
) -> Result<CommitPasteReturn, crate::ipc::IpcError> {
    let payload = l3
        .completed
        .lock()
        .unwrap()
        .remove(&stream_id)
        .ok_or_else(|| {
            crate::ipc::IpcError::new("L3_CONFIG_MISSING", "stream 不存在或已上屏")
        })?;
    write_plain_to_pasteboard(&payload.plain_text, engine.clipboard_owner.clone());
    Ok(CommitPasteReturn { mode: "COPIED_WAIT_PASTE" })
}

/// Ollama 探测（主面板 Tab 3，UI Spec §5.2）
#[tauri::command]
pub fn cmd_probe_ollama(base_url: String) -> crate::l3::OllamaProbeResult {
    crate::l3::probe_ollama(&base_url)
}

/// 保存 BYOK 配置（主面板 Tab 3）
#[tauri::command]
pub fn cmd_set_llm_config(
    l3: tauri::State<'_, std::sync::Arc<L3IpcState>>,
    base_url: String,
    api_key: String,
    model: String,
) -> SetLlmConfigReturn {
    l3.runtime.set_config(LlmConfig {
        base_url,
        api_key,
        model,
    });
    SetLlmConfigReturn { saved: true }
}

/// 一次性原子写入纯文本（TOCTOU: clearContents 后立即 setString；登记内部写入防误判）
#[cfg(target_os = "macos")]
pub fn write_plain_to_pasteboard(text: &str, owner: std::sync::Arc<crate::clipboard_owner::ClipboardOwner>) {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
    use objc2_foundation::NSString;
    unsafe {
        let pb = NSPasteboard::generalPasteboard();
        let _ = pb.clearContents();
        let ns = NSString::from_str(text);
        pb.setString_forType(&ns, NSPasteboardTypeString);
        let cc = pb.changeCount() as i64;
        owner.register_internal_write(cc); // 防 DoubleCopyDetector 误判
    }
}

#[cfg(not(target_os = "macos"))]
pub fn write_plain_to_pasteboard(
    _text: &str,
    _owner: std::sync::Arc<crate::clipboard_owner::ClipboardOwner>,
) {
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_user_content_appends_source_after_prefix() {
        let c = build_user_content("你是助手", "原文内容");
        assert!(c.starts_with("你是助手"));
        assert!(c.contains("原文内容"));
    }

    #[test]
    fn dlp_blocks_flags_secret_not_plain() {
        assert!(dlp_blocks("token sk-abc123def456ghi789jkl012mno"));
        assert!(!dlp_blocks("普通会议纪要文本"));
    }

    #[test]
    fn stream_id_is_unique_and_prefixed() {
        let a = new_stream_id();
        let b = new_stream_id();
        assert!(a.starts_with("l3-"));
        assert_ne!(a, b);
    }

    #[test]
    fn runtime_stores_config() {
        let rt = L3Runtime::new();
        assert!(rt.config().is_none());
        rt.set_config(LlmConfig {
            base_url: "http://127.0.0.1:11434/v1".into(),
            api_key: "".into(),
            model: "qwen3:8b".into(),
        });
        assert_eq!(rt.config().unwrap().model, "qwen3:8b");
    }

    #[test]
    fn completed_payload_take_semantics() {
        // commit 后不可重复上屏 (take)
        let st = L3IpcState::new();
        let payload = crate::l3::AtomicPastePayload {
            plain_text: "out".into(),
            first_chunk_ms: 10,
            total_ms: 20,
            chunk_count: 1,
        };
        st.completed.lock().unwrap().insert("s1".into(), payload);
        let first = st.completed.lock().unwrap().remove("s1");
        assert!(first.is_some());
        assert!(st.completed.lock().unwrap().remove("s1").is_none());
    }
}
