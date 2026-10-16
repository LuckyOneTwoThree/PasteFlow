//! P3-1 L3 流式引擎 (Master Spec §5.3 / Dev Plan P3-1)
//!
//! OpenAI 兼容流式聚合（BYOK + 本地 Ollama）：
//!   - 首块延迟 < 600ms 目标（UI Spec §3.3 流式预览）
//!   - `Esc` 中止即时熔断（`cmd_cancel_l3_stream`）
//!   - 生成完毕**一次性原子粘贴**（避免撤销栈碎裂，R-5）
//!   - 未配置 Key → `L3_CONFIG_MISSING`
//!
//! 网络层抽象为 `L3Provider` trait（测试用假实现），真实 HTTP SSE 由 P3 网络层接入。
//!
//! 安全铁律（Master Spec §8.3）：
//!   - DLP 命中密钥的剪贴板内容**禁止送入 L3 云端接口**（`DlpVerdict::SilentBypass`）
//!   - 只读安全前缀锁定（防 Prompt Injection，UI Spec §5.1）

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// L3 错误码 (IPC Spec §3.3 扩展)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum L3Error {
    /// 未配置 Base URL / API Key / 模型名
    ConfigMissing,
    /// DLP 敏感凭证禁止送云端
    DlpBlocked,
    /// 用户中途 Esc / cancel
    Cancelled,
    /// Provider 返回错误
    ProviderFailed(String),
    /// 超时
    Timeout,
}

impl L3Error {
    pub fn code(&self) -> &'static str {
        match self {
            L3Error::ConfigMissing => "L3_CONFIG_MISSING",
            L3Error::DlpBlocked => "L3_DLP_BLOCKED",
            L3Error::Cancelled => "L3_CANCELLED",
            L3Error::ProviderFailed(_) => "L3_PROVIDER_FAILED",
            L3Error::Timeout => "L3_TIMEOUT",
        }
    }
}

// —— BYOK 配置 ——

/// L3 模型配置 (IPC Spec `LlmConfigDto` / UI Spec §5.2 Tab 3)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfig {
    /// OpenAI 兼容 Base URL (本地 Ollama: `http://127.0.0.1:11434/v1`)
    pub base_url: String,
    /// BYOK API Key (本地 Ollama 可为空)
    pub api_key: String,
    /// 模型名称 (如 `qwen3:8b`)
    pub model: String,
}

impl LlmConfig {
    /// 配置有效性校验: 未配置 → `L3_CONFIG_MISSING`
    ///
    /// 本地 Ollama（loopback 且含 `/v1`）允许空 Key。
    pub fn validate(&self) -> Result<(), L3Error> {
        if self.base_url.trim().is_empty() || self.model.trim().is_empty() {
            return Err(L3Error::ConfigMissing);
        }
        let is_local = self.base_url.starts_with("http://127.0.0.1")
            || self.base_url.starts_with("http://localhost");
        if !is_local && self.api_key.trim().is_empty() {
            return Err(L3Error::ConfigMissing);
        }
        Ok(())
    }
}

// —— 流式会话状态机 ——

/// 会话阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StreamPhase {
    /// 尚未启动
    Idle,
    /// 流式接收中（HUD 实时预览）
    Streaming,
    /// 用户 Esc 中止
    Cancelled,
    /// 生成完毕，待一次性原子粘贴
    Completed,
    /// 失败
    Failed,
}

/// 流式增量（HUD 预览用）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamDelta {
    pub index: usize,
    pub text: String,
}

/// 原子粘贴载荷（生成完毕一次性写入）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AtomicPastePayload {
    pub plain_text: String,
    /// 首块延迟 (ms, UI Spec §3.3 预算 < 600ms)
    pub first_chunk_ms: u64,
    /// 总耗时 (ms)
    pub total_ms: u64,
    pub chunk_count: usize,
}

/// L3 流式 Provider 抽象（网络层实现；测试用假实现）
///
/// 流式增量经 `on_chunk` 回调交付；回调返回 `false`（用户 Esc 中止）
/// 时 Provider 必须立即停止读取网络流（即时熔断，Dev Plan P3-1）。
pub trait L3Provider: Send + Sync {
    fn stream(
        &self,
        system_prompt: &str,
        user_content: &str,
        on_chunk: &mut dyn FnMut(&str) -> bool,
    ) -> Result<(), L3Error>;
}

/// L3 流式会话状态机
#[derive(Debug)]
pub struct L3Session {
    phase: StreamPhase,
    /// 已累积的预览文本
    buffer: String,
    /// 分块计数
    chunk_count: usize,
    /// 首块延迟 (ms)
    first_chunk_ms: Option<u64>,
    /// 已中止标记（Esc 熔断）
    cancelled: bool,
    /// 系统安全前缀（只读锁定，防 Prompt Injection）
    system_prefix: String,
}

impl L3Session {
    /// 新建会话（安全前缀锁定，UI Spec §5.1）
    pub fn new(system_prefix: &str) -> Self {
        Self {
            phase: StreamPhase::Idle,
            buffer: String::new(),
            chunk_count: 0,
            first_chunk_ms: None,
            cancelled: false,
            system_prefix: system_prefix.to_string(),
        }
    }

    pub fn phase(&self) -> StreamPhase {
        self.phase
    }

    pub fn preview(&self) -> &str {
        &self.buffer
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    /// Esc / `cmd_cancel_l3_stream` 中止（即时熔断）
    pub fn cancel(&mut self) {
        if self.phase == StreamPhase::Streaming {
            self.cancelled = true;
            self.phase = StreamPhase::Cancelled;
        }
    }

    /// 喂入流式增量（HUD 实时预览）
    ///
    /// 中止后忽略后续块（熔断）。
    pub fn feed(&mut self, delta: &str, elapsed_ms: u64) -> bool {
        if self.cancelled || self.phase == StreamPhase::Cancelled {
            return false;
        }
        if self.phase == StreamPhase::Idle {
            self.phase = StreamPhase::Streaming;
        }
        if self.chunk_count == 0 {
            self.first_chunk_ms = Some(elapsed_ms);
        }
        self.chunk_count += 1;
        self.buffer.push_str(delta);
        true
    }

    /// 流式完成 → 一次性原子粘贴载荷（R-5: 避免撤销栈碎裂）
    pub fn finish(&mut self) -> Result<AtomicPastePayload, L3Error> {
        self.finish_at(self.first_chunk_ms.unwrap_or(0))
    }

    /// 流式完成（带真实总耗时）
    pub fn finish_at(&mut self, total_ms: u64) -> Result<AtomicPastePayload, L3Error> {
        if self.cancelled {
            return Err(L3Error::Cancelled);
        }
        if self.phase != StreamPhase::Streaming {
            return Err(L3Error::ProviderFailed("no streaming data".into()));
        }
        self.phase = StreamPhase::Completed;
        Ok(AtomicPastePayload {
            plain_text: std::mem::take(&mut self.buffer),
            first_chunk_ms: self.first_chunk_ms.unwrap_or(0),
            total_ms,
            chunk_count: self.chunk_count,
        })
    }

    pub fn system_prefix(&self) -> &str {
        &self.system_prefix
    }
}

// —— L3 滤镜配方 (P3-2: 贪吃蛇智能汇总 / 得体打工人) ——

/// 汇总提示词类型 (IPC Spec `summaryPromptType`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PromptType {
    MeetingNotes,
    ComparisonTable,
    TodoList,
    /// 得体打工人滤镜 (P3-2): 让语气更委婉专业
    PoliteTone,
}

/// L3 滤镜配方注册表
#[derive(Debug, Default)]
pub struct L3Recipes {
    prompts: HashMap<PromptType, &'static str>,
}

impl L3Recipes {
    /// 内置配方（只读安全前缀锁定，UI Spec §5.1）
    pub fn with_builtin() -> Self {
        let mut r = Self::default();
        r.prompts.insert(
            PromptType::MeetingNotes,
            "你是会议纪要助手。仅基于用户提供的文本输出结构化纪要，不得虚构内容。格式：## 会议纪要 / ### 议题 / ### 决议 / ### 待办。",
        );
        r.prompts.insert(
            PromptType::ComparisonTable,
            "你是数据对比助手。仅基于用户提供的文本生成 Markdown 对比表格，不得虚构内容。",
        );
        r.prompts.insert(
            PromptType::TodoList,
            "你是待办整理助手。仅基于用户提供的文本输出 Markdown 待办清单（- [ ] 格式），不得虚构内容。",
        );
        r.prompts.insert(
            PromptType::PoliteTone,
            "你是职场沟通助手。在不改变原意的前提下，将用户文本改写得更委婉专业，不得添加新信息。",
        );
        r
    }

    pub fn prompt(&self, kind: PromptType) -> Option<&'static str> {
        self.prompts.get(&kind).copied()
    }
}

// —— L3 执行编排（DLP 防护 + Provider 调用） ——

/// L3 执行入口：校验配置 → DLP 防护 → 流式聚合
///
/// `dlp_blocked = true` 时直接拒绝（Master Spec §8.3: DLP 命中密钥禁止送云端）。
/// 会话被 `cancel` 后回调立即返回 `false`，Provider 熔断网络流（R-5）。
/// `on_delta` 返回 `false` 亦触发熔断（外部取消标志，如 `cmd_cancel_l3_stream`）。
pub fn run_l3(
    provider: &dyn L3Provider,
    config: &LlmConfig,
    system_prompt: &str,
    user_content: &str,
    dlp_blocked: bool,
    session: &mut L3Session,
    mut on_delta: Option<&mut dyn FnMut(usize, &str, &str) -> bool>, // (chunk_index, delta, accumulated)
) -> Result<AtomicPastePayload, L3Error> {
    config.validate()?;
    if dlp_blocked {
        return Err(L3Error::DlpBlocked);
    }
    let started = std::time::Instant::now();
    let mut idx = 0usize;
    let mut cb = |delta: &str| -> bool {
        if session.is_cancelled() {
            return false;
        }
        session.feed(delta, started.elapsed().as_millis() as u64);
        if let Some(cb) = on_delta.as_deref_mut() {
            if !cb(idx, delta, session.preview()) {
                session.cancel(); // 外部熔断: 同步置会话为 Cancelled
                return false;
            }
        }
        idx += 1;
        true
    };
    provider.stream(system_prompt, user_content, &mut cb)?;
    if session.is_cancelled() {
        return Err(L3Error::Cancelled);
    }
    session.finish_at(started.elapsed().as_millis() as u64)
}

/// Ollama 本地探测 (UI Spec §5.2 `[探测 Ollama 连接]`)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaProbeResult {
    pub ok: bool,
    pub models: Vec<String>,
}

/// 探测本地 Ollama（GET {base}/v1/models，3s 超时）
///
/// HTTP 失败/超时返回 `ok: false`（前端显示「Ollama 未检测到」）。
pub fn probe_ollama(base_url: &str) -> OllamaProbeResult {
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(3))
        .build();
    match agent.get(&url).call() {
        Ok(resp) => {
            let models = resp
                .into_json::<serde_json::Value>()
                .ok()
                .and_then(|v| {
                    v.get("data").and_then(|d| {
                        d.as_array().map(|arr| {
                            arr.iter()
                                .filter_map(|m| m.get("id").and_then(|i| i.as_str()).map(String::from))
                                .collect::<Vec<_>>()
                        })
                    })
                })
                .unwrap_or_default();
            OllamaProbeResult { ok: true, models }
        }
        Err(_) => OllamaProbeResult { ok: false, models: Vec::new() },
    }
}

// —— P3-1 收尾: SSE 解析 + OpenAI 兼容 HTTP Provider + 流管理器 ——

/// SSE 事件解析器（增量喂入 `data:` 行，吐出 content delta）
///
/// 兼容 OpenAI 兼容端点（`chat.completions` chunk 的 `choices[0].delta.content`）
/// 与 Ollama 格式（`message.content` / `response`）。`[DONE]` 结束标记置 `done`。
#[derive(Debug, Default)]
pub struct SseParser {
    buf: String,
    pub done: bool,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// 喂入原始网络字节片段，返回本片段解析出的 content delta
    pub fn feed(&mut self, bytes: &str) -> Vec<String> {
        self.buf.push_str(bytes);
        let mut deltas = Vec::new();
        // SSE 事件以空行分隔；行以 \n 或 \r\n 结尾
        while let Some(pos) = self.buf.find('\n') {
            let line = self.buf[..pos].trim_end_matches('\r').to_string();
            self.buf.drain(..=pos);
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data == "[DONE]" {
                self.done = true;
                continue;
            }
            if let Some(delta) = extract_sse_delta(data) {
                if !delta.is_empty() {
                    deltas.push(delta);
                }
            }
        }
        deltas
    }
}

/// 从 SSE data JSON 提取 content delta（多端点兼容）
fn extract_sse_delta(data: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(data).ok()?;
    // OpenAI 兼容: choices[0].delta.content (流式) 或 choices[0].message.content (非流式回退)
    if let Some(choices) = v.get("choices").and_then(|c| c.as_array()) {
        if let Some(first) = choices.first() {
            if let Some(c) = first
                .get("delta")
                .and_then(|d| d.get("content"))
                .and_then(|c| c.as_str())
            {
                return Some(c.to_string());
            }
            if let Some(c) = first
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
            {
                return Some(c.to_string());
            }
        }
    }
    // Ollama /api/generate 格式: response
    if let Some(c) = v.get("response").and_then(|c| c.as_str()) {
        return Some(c.to_string());
    }
    None
}

/// OpenAI 兼容 HTTP 流式 Provider（BYOK + 本地 Ollama 共用）
pub struct HttpProvider {
    pub config: LlmConfig,
    /// 连接/首字超时（秒）
    pub timeout_secs: u64,
}

impl HttpProvider {
    pub fn new(config: LlmConfig) -> Self {
        Self { config, timeout_secs: 30 }
    }

    /// 组装 chat.completions 请求体（流式）
    fn request_body(&self, system_prompt: &str, user_content: &str) -> serde_json::Value {
        serde_json::json!({
            "model": self.config.model,
            "stream": true,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_content }
            ]
        })
    }
}

impl L3Provider for HttpProvider {
    fn stream(
        &self,
        system_prompt: &str,
        user_content: &str,
        on_chunk: &mut dyn FnMut(&str) -> bool,
    ) -> Result<(), L3Error> {
        self.config.validate()?;
        let url = format!("{}/chat/completions", self.config.base_url.trim_end_matches('/'));
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(self.timeout_secs))
            .build();
        let mut req = agent
            .post(&url)
            .set("Content-Type", "application/json")
            .set("Accept", "text/event-stream");
        if !self.config.api_key.trim().is_empty() {
            req = req.set("Authorization", &format!("Bearer {}", self.config.api_key.trim()));
        }
        let resp = req
            .send_json(self.request_body(system_prompt, user_content))
            .map_err(|e| L3Error::ProviderFailed(e.to_string()))?;
        use std::io::Read;
        let mut reader = resp.into_reader().take(8 * 1024 * 1024); // 8MB 流上限
        let mut parser = SseParser::new();
        let mut buf = [0u8; 4096];
        loop {
            let n = reader
                .read(&mut buf)
                .map_err(|e| L3Error::ProviderFailed(e.to_string()))?;
            if n == 0 {
                break;
            }
            let chunk = String::from_utf8_lossy(&buf[..n]).into_owned();
            for delta in parser.feed(&chunk) {
                if !on_chunk(&delta) {
                    return Err(L3Error::Cancelled); // Esc 熔断: 立即丢弃网络流
                }
            }
            if parser.done {
                break;
            }
        }
        Ok(())
    }
}

/// 流式会话注册表（`cmd_cancel_l3_stream` 按 streamId 即时熔断）
#[derive(Debug, Default)]
pub struct StreamRegistry {
    streams: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl StreamRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记新流，返回取消标志句柄（后台流每块前检查）
    pub fn register(&self, stream_id: &str) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        self.streams.lock().unwrap().insert(stream_id.to_string(), flag.clone());
        flag
    }

    /// 取消（Esc / `cmd_cancel_l3_stream`）→ 即时熔断
    pub fn cancel(&self, stream_id: &str) -> bool {
        if let Some(flag) = self.streams.lock().unwrap().remove(stream_id) {
            flag.store(true, Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    /// 流结束移除登记
    pub fn finish(&self, stream_id: &str) {
        self.streams.lock().unwrap().remove(stream_id);
    }

    pub fn active_count(&self) -> usize {
        self.streams.lock().unwrap().len()
    }
}

/// L3 运行时状态（BYOK 配置 + 流注册表，由 lib.rs manage）
#[derive(Debug, Default)]
pub struct L3Runtime {
    pub registry: StreamRegistry,
    config: Mutex<Option<LlmConfig>>,
}

impl L3Runtime {
    pub fn new() -> Self {
        Self::default()
    }

    /// 保存 BYOK 配置（主面板 Tab 3 设置）
    pub fn set_config(&self, cfg: LlmConfig) {
        *self.config.lock().unwrap() = Some(cfg);
    }

    pub fn config(&self) -> Option<LlmConfig> {
        self.config.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeProvider {
        chunks: Vec<String>,
    }

    impl L3Provider for FakeProvider {
        fn stream(
            &self,
            _s: &str,
            _u: &str,
            on_chunk: &mut dyn FnMut(&str) -> bool,
        ) -> Result<(), L3Error> {
            for c in &self.chunks {
                if !on_chunk(c) {
                    return Err(L3Error::Cancelled);
                }
            }
            Ok(())
        }
    }

    #[test]
    fn config_missing_without_base_url() {
        let c = LlmConfig { base_url: "".into(), api_key: "k".into(), model: "m".into() };
        assert_eq!(c.validate(), Err(L3Error::ConfigMissing));
        assert_eq!(L3Error::ConfigMissing.code(), "L3_CONFIG_MISSING");
    }

    #[test]
    fn cloud_config_requires_api_key() {
        let c = LlmConfig { base_url: "https://api.openai.com/v1".into(), api_key: "".into(), model: "gpt".into() };
        assert_eq!(c.validate(), Err(L3Error::ConfigMissing));
    }

    #[test]
    fn local_ollama_allows_empty_key() {
        let c = LlmConfig {
            base_url: "http://127.0.0.1:11434/v1".into(),
            api_key: "".into(),
            model: "qwen3:8b".into(),
        };
        assert!(c.validate().is_ok());
    }

    #[test]
    fn dlp_blocked_rejects_cloud() {
        let p = FakeProvider { chunks: vec!["x".into()] };
        let c = LlmConfig { base_url: "http://127.0.0.1:11434/v1".into(), api_key: "".into(), model: "m".into() };
        let mut s = L3Session::new("prefix");
        let r = run_l3(&p, &c, "sys", "user", true, &mut s, None);
        assert_eq!(r, Err(L3Error::DlpBlocked));
        assert_eq!(L3Error::DlpBlocked.code(), "L3_DLP_BLOCKED");
    }

    #[test]
    fn streaming_feeds_and_completes_atomically() {
        let p = FakeProvider { chunks: vec!["会议".into(), "纪要".into(), "完成".into()] };
        let c = LlmConfig { base_url: "http://127.0.0.1:11434/v1".into(), api_key: "".into(), model: "m".into() };
        let mut s = L3Session::new("prefix");
        let r = run_l3(&p, &c, "sys", "user", false, &mut s, None).unwrap();
        assert_eq!(r.plain_text, "会议纪要完成");
        assert_eq!(r.chunk_count, 3);
        // 首块延迟 < 600ms (UI Spec §3.3); 本地 FakeProvider 为即时投递
        assert!(r.first_chunk_ms < 600);
        assert_eq!(s.phase(), StreamPhase::Completed);
    }

    #[test]
    fn on_delta_callback_receives_accumulated() {
        let p = FakeProvider { chunks: vec!["a".into(), "b".into()] };
        let c = LlmConfig { base_url: "http://127.0.0.1:11434/v1".into(), api_key: "".into(), model: "m".into() };
        let mut s = L3Session::new("prefix");
        let mut seen: Vec<(usize, String, String)> = Vec::new();
        {
            let mut cb = |i: usize, d: &str, acc: &str| -> bool {
                seen.push((i, d.to_string(), acc.to_string()));
                true
            };
            run_l3(&p, &c, "sys", "user", false, &mut s, Some(&mut cb)).unwrap();
        }
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0], (0, "a".into(), "a".into()));
        assert_eq!(seen[1], (1, "b".into(), "ab".into()));
    }

    #[test]
    fn on_delta_returning_false_fuses() {
        // 外部熔断 (cmd_cancel_l3_stream 标志): 回调返回 false -> 立即停止
        let p = FakeProvider { chunks: vec!["a".into(), "b".into(), "c".into()] };
        let c = LlmConfig { base_url: "http://127.0.0.1:11434/v1".into(), api_key: "".into(), model: "m".into() };
        let mut s = L3Session::new("prefix");
        let r = {
            let mut cb = |_i: usize, _d: &str, _acc: &str| -> bool { false };
            run_l3(&p, &c, "sys", "user", false, &mut s, Some(&mut cb))
        };
        assert_eq!(r, Err(L3Error::Cancelled));
        assert_eq!(s.phase(), StreamPhase::Cancelled);
    }

    #[test]
    fn cancel_mid_stream_fuses() {
        let mut s = L3Session::new("prefix");
        s.feed("hello", 50);
        s.cancel();
        assert_eq!(s.phase(), StreamPhase::Cancelled);
        // 中止后忽略后续块 (熔断)
        assert!(!s.feed("world", 60));
        assert_eq!(s.preview(), "hello");
        assert_eq!(s.finish(), Err(L3Error::Cancelled));
    }

    #[test]
    fn finish_without_data_fails() {
        let mut s = L3Session::new("prefix");
        assert!(matches!(s.finish(), Err(L3Error::ProviderFailed(_))));
    }

    #[test]
    fn builtin_recipes_cover_p3_2() {
        let r = L3Recipes::with_builtin();
        // P3-2: 贪吃蛇智能汇总 (3 类) + 得体打工人
        assert!(r.prompt(PromptType::MeetingNotes).is_some());
        assert!(r.prompt(PromptType::ComparisonTable).is_some());
        assert!(r.prompt(PromptType::TodoList).is_some());
        assert!(r.prompt(PromptType::PoliteTone).is_some());
    }

    #[test]
    fn prompt_prefixes_are_read_only_locked() {
        // 安全前缀: 不得虚构 (防 Prompt Injection, UI Spec §5.1)
        let r = L3Recipes::with_builtin();
        for k in [PromptType::MeetingNotes, PromptType::PoliteTone] {
            assert!(r.prompt(k).unwrap().contains("不得"));
        }
    }

    #[test]
    fn session_locks_system_prefix() {
        let s = L3Session::new("只读安全前缀");
        assert_eq!(s.system_prefix(), "只读安全前缀");
    }

    // —— SSE 解析 (P3-1 网络层) ——

    #[test]
    fn sse_parser_extracts_openai_deltas() {
        let mut p = SseParser::new();
        let deltas = p.feed(
            "data: {\"choices\":[{\"delta\":{\"content\":\"你好\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"世界\"}}]}\n\n",
        );
        assert_eq!(deltas, vec!["你好".to_string(), "世界".to_string()]);
        assert!(!p.done);
    }

    #[test]
    fn sse_parser_handles_done_and_split_lines() {
        let mut p = SseParser::new();
        // 行跨两个网络片段到达
        assert!(p.feed("data: {\"choices\":[{\"delta\":{\"content\":\"ab\"}}]").is_empty());
        let d = p.feed("}\n\ndata: [DONE]\n\n");
        assert_eq!(d, vec!["ab".to_string()]);
        assert!(p.done);
    }

    #[test]
    fn sse_parser_supports_ollama_response_field() {
        let mut p = SseParser::new();
        let d = p.feed("data: {\"response\":\"部分\"}\n\n");
        assert_eq!(d, vec!["部分".to_string()]);
    }

    #[test]
    fn sse_parser_ignores_empty_delta_and_comments() {
        let mut p = SseParser::new();
        let d = p.feed(": comment\ndata: {\"choices\":[{\"delta\":{\"content\":\"\"}}]}\n\n");
        assert!(d.is_empty());
    }

    // —— 流注册表 (cmd_cancel_l3_stream 即时熔断) ——

    #[test]
    fn stream_registry_register_cancel_finish() {
        let reg = StreamRegistry::new();
        let flag = reg.register("s1");
        assert_eq!(reg.active_count(), 1);
        assert!(!flag.load(Ordering::SeqCst));
        assert!(reg.cancel("s1"));
        assert!(flag.load(Ordering::SeqCst)); // 熔断标志置位
        assert_eq!(reg.active_count(), 0);
        assert!(!reg.cancel("s1")); // 已移除
        reg.register("s2");
        reg.finish("s2");
        assert_eq!(reg.active_count(), 0);
    }
}
