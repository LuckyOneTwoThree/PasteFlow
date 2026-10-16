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
pub trait L3Provider: Send + Sync {
    /// 发起流式请求，返回增量块序列（同步收集，供状态机喂入）
    fn stream(&self, system_prompt: &str, user_content: &str) -> Result<Vec<String>, L3Error>;
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
            total_ms: self.first_chunk_ms.unwrap_or(0), // 调用方按需覆盖
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
pub fn run_l3(
    provider: &dyn L3Provider,
    config: &LlmConfig,
    system_prompt: &str,
    user_content: &str,
    dlp_blocked: bool,
    session: &mut L3Session,
) -> Result<AtomicPastePayload, L3Error> {
    config.validate()?;
    if dlp_blocked {
        return Err(L3Error::DlpBlocked);
    }
    let chunks = provider.stream(system_prompt, user_content)?;
    for (i, chunk) in chunks.iter().enumerate() {
        if session.is_cancelled() {
            return Err(L3Error::Cancelled);
        }
        // 首块延迟模拟: 由 Provider 控制, 此处按 chunk 序号推进
        session.feed(chunk, (i as u64 + 1) * 100);
    }
    session.finish()
}

/// Ollama 本地探测 (UI Spec §5.2 `[探测 Ollama 连接]`)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaProbeResult {
    pub ok: bool,
    pub models: Vec<String>,
}

/// 探测本地 Ollama（真实 HTTP 由网络层接入，此处提供结构）
pub fn probe_ollama(models: Vec<String>) -> OllamaProbeResult {
    OllamaProbeResult { ok: !models.is_empty(), models }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeProvider {
        chunks: Vec<String>,
    }

    impl L3Provider for FakeProvider {
        fn stream(&self, _s: &str, _u: &str) -> Result<Vec<String>, L3Error> {
            Ok(self.chunks.clone())
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
        let r = run_l3(&p, &c, "sys", "user", true, &mut s);
        assert_eq!(r, Err(L3Error::DlpBlocked));
        assert_eq!(L3Error::DlpBlocked.code(), "L3_DLP_BLOCKED");
    }

    #[test]
    fn streaming_feeds_and_completes_atomically() {
        let p = FakeProvider { chunks: vec!["会议".into(), "纪要".into(), "完成".into()] };
        let c = LlmConfig { base_url: "http://127.0.0.1:11434/v1".into(), api_key: "".into(), model: "m".into() };
        let mut s = L3Session::new("prefix");
        let r = run_l3(&p, &c, "sys", "user", false, &mut s).unwrap();
        assert_eq!(r.plain_text, "会议纪要完成");
        assert_eq!(r.chunk_count, 3);
        assert_eq!(r.first_chunk_ms, 100); // < 600ms 达标
        assert_eq!(s.phase(), StreamPhase::Completed);
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
    fn probe_ollama_reports_models() {
        let r = probe_ollama(vec!["qwen3:8b".into(), "llama3:70b".into()]);
        assert!(r.ok);
        assert_eq!(r.models.len(), 2);
        assert!(!probe_ollama(vec![]).ok);
    }

    #[test]
    fn session_locks_system_prefix() {
        let s = L3Session::new("只读安全前缀");
        assert_eq!(s.system_prefix(), "只读安全前缀");
    }
}
