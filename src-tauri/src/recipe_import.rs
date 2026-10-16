//! P2-9 社区配方安全导入 (IPC Spec v1.0 §5 `schema/pipe-v1.json` / `cmd_import_pipe_recipe`)
//!
//! 严格校验规则（对齐 JSON Schema 约束，`additionalProperties: false` 由
//! `#[serde(deny_unknown_fields)]` 实现）：
//!   - 全层级拒绝未知字段
//!   - `providerTimeoutMs` 上限锁死 `8ms`
//!   - `regex_pipeline` 的 `steps` 预编译验证（可编译 + flags 仅 `imsU`）
//!   - `engine == "llm_stream"` 必须经用户显式确认 Prompt 审查
//!
//! 错误码（IPC Spec §3.3）：
//!   - `RECIPE_SCHEMA_INVALID`：Schema 校验或正则预编译失败
//!   - `PROMPT_REVIEW_REQUIRED`：llm_stream 配方未确认审查

use regex::Regex;
use serde::Deserialize;

/// 导入错误（对应 IPC 错误码）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    /// Schema 校验或正则预编译失败
    SchemaInvalid(String),
    /// llm_stream 配方需用户显式确认 Prompt 审查
    PromptReviewRequired,
}

impl ImportError {
    /// IPC 错误码 (IPC Spec §3.3)
    pub fn code(&self) -> &'static str {
        match self {
            ImportError::SchemaInvalid(_) => "RECIPE_SCHEMA_INVALID",
            ImportError::PromptReviewRequired => "PROMPT_REVIEW_REQUIRED",
        }
    }
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::SchemaInvalid(msg) => write!(f, "RECIPE_SCHEMA_INVALID: {msg}"),
            ImportError::PromptReviewRequired => {
                write!(f, "PROMPT_REVIEW_REQUIRED: llm_stream recipe requires prompt review confirmation")
            }
        }
    }
}

impl std::error::Error for ImportError {}

// —— Schema 数据结构（deny_unknown_fields = additionalProperties: false）——

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommunityRecipe {
    #[serde(rename = "$schema")]
    pub schema: String,
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub version: String,
    pub icon: String,
    pub category: String,
    pub tier: String,
    pub trigger: TriggerSpec,
    pub processor: ProcessorSpec,
    pub output: OutputSpec,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TriggerSpec {
    pub mode: String,
    #[serde(default)]
    pub match_content_tags: Vec<String>,
    #[serde(default)]
    pub match_source_apps: Vec<String>,
    pub own_channels: Vec<String>,
    pub min_confidence: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProcessorSpec {
    pub engine: String,
    pub handler_name: String,
    #[serde(default)]
    pub steps: Vec<RegexStep>,
    pub fallback_prompt: Option<String>,
    pub provider_timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegexStep {
    pub pattern: String,
    #[serde(default)]
    pub replacement: String,
    #[serde(default)]
    pub flags: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct OutputSpec {
    pub channels: ChannelSpec,
    pub plain_text_policy: String,
    pub preserve_original: bool,
    #[serde(default)]
    pub toast_messages: Option<ToastMessages>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ChannelSpec {
    #[serde(default)]
    pub html: Option<ChannelHandler>,
    #[serde(default)]
    pub rtf: Option<ChannelHandler>,
    pub plain_text: PlainTextHandler,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ChannelHandler {
    pub handler: String,
    #[serde(default)]
    pub suppress_for_target_bundles: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PlainTextHandler {
    pub default_handler: String,
    #[serde(default)]
    pub im_target_handler: Option<String>,
    #[serde(default)]
    pub im_target_bundles: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToastMessages {
    #[serde(rename = "DYNAMIC_BY_TARGET", default)]
    pub dynamic_by_target: Option<String>,
    #[serde(rename = "PASSTHROUGH", default)]
    pub passthrough: Option<String>,
    #[serde(rename = "CLEANED", default)]
    pub cleaned: Option<String>,
    #[serde(rename = "SANITIZED_OPT_IN", default)]
    pub sanitized_opt_in: Option<String>,
}

// —— 枚举约束常量（对齐 IPC Spec §5）——

const CATEGORIES: &[&str] = &["office", "developer", "cleaning", "privacy", "vision", "ai_writing"];
const TIERS: &[&str] = &["L1_LOCAL", "L2_VISION", "L3_STREAMING_AI"];
const TRIGGER_MODES: &[&str] = &["AUTO_AND_MANUAL", "MANUAL_ONLY", "OFF"];
const CONTENT_TAGS: &[&str] = &[
    "markdown", "dirty_html", "url_with_tracker", "pdf_broken_lines", "json",
    "curl", "code", "contains_secret", "table_image",
];
const OWN_CHANNELS: &[&str] = &["html", "rtf", "plainText"];
const ENGINES: &[&str] = &["builtin_rust_handler", "regex_pipeline", "llm_stream"];
const POLICIES: &[&str] = &["DYNAMIC_BY_TARGET", "PASSTHROUGH", "CLEANED", "SANITIZED_OPT_IN"];

const MAX_STEPS: usize = 16;
const MAX_PATTERN_LEN: usize = 512;
const MAX_REPLACEMENT_LEN: usize = 1024;
const MAX_FALLBACK_PROMPT_LEN: usize = 4096;
const MAX_PROVIDER_TIMEOUT_MS: u64 = 8;

/// 严格校验社区配方 (IPC Spec §5)
///
/// 返回通过校验的配方；任何违反 Schema / 正则预编译失败 → `RECIPE_SCHEMA_INVALID`。
/// `llm_stream` 配方且 `user_confirmed_prompt_review == false` → `PROMPT_REVIEW_REQUIRED`。
pub fn import_recipe(
    raw_json: &str,
    user_confirmed_prompt_review: bool,
) -> Result<CommunityRecipe, ImportError> {
    // 解析 + 未知字段拒绝 (additionalProperties: false)
    let recipe: CommunityRecipe = serde_json::from_str(raw_json)
        .map_err(|e| ImportError::SchemaInvalid(format!("JSON schema violation: {e}")))?;

    validate_fields(&recipe)?;

    // llm_stream 强制 Prompt 审查 (UI Spec §5.1 防 Prompt Injection)
    if recipe.processor.engine == "llm_stream" && !user_confirmed_prompt_review {
        return Err(ImportError::PromptReviewRequired);
    }

    Ok(recipe)
}

fn validate_fields(r: &CommunityRecipe) -> Result<(), ImportError> {
    let bad = |m: &str| ImportError::SchemaInvalid(m.to_string());

    // id: ^[a-z0-9_]+(\.[a-z0-9_-]+)+$
    let id_re = Regex::new(r"^[a-z0-9_]+(\.[a-z0-9_-]+)+$").expect("static regex");
    if !id_re.is_match(&r.id) {
        return Err(bad("id must match ^[a-z0-9_]+(\\.[a-z0-9_-]+)+$"));
    }
    if !(2..=64).contains(&r.name.chars().count()) {
        return Err(bad("name must be 2..=64 chars"));
    }
    if !(4..=240).contains(&r.description.chars().count()) {
        return Err(bad("description must be 4..=240 chars"));
    }
    if !(1..=64).contains(&r.author.chars().count()) {
        return Err(bad("author must be 1..=64 chars"));
    }
    let ver_re = Regex::new(r"^\d+\.\d+\.\d+$").expect("static regex");
    if !ver_re.is_match(&r.version) {
        return Err(bad("version must be semver x.y.z"));
    }
    if !CATEGORIES.contains(&r.category.as_str()) {
        return Err(bad("invalid category"));
    }
    if !TIERS.contains(&r.tier.as_str()) {
        return Err(bad("invalid tier"));
    }

    // trigger
    let t = &r.trigger;
    if !TRIGGER_MODES.contains(&t.mode.as_str()) {
        return Err(bad("invalid trigger.mode"));
    }
    if t.match_content_tags.is_empty() {
        return Err(bad("trigger.matchContentTags must have >= 1 item"));
    }
    for tag in &t.match_content_tags {
        if !CONTENT_TAGS.contains(&tag.as_str()) {
            return Err(bad("invalid content tag"));
        }
    }
    if t.match_source_apps.is_empty() {
        return Err(bad("trigger.matchSourceApps must have >= 1 item"));
    }
    if t.own_channels.is_empty() {
        return Err(bad("trigger.ownChannels must have >= 1 item"));
    }
    for ch in &t.own_channels {
        if !OWN_CHANNELS.contains(&ch.as_str()) {
            return Err(bad("invalid ownChannel"));
        }
    }
    // ownChannels 必含 plainText 且去重 (JSON Schema contains + uniqueItems)
    if !t.own_channels.iter().any(|c| c == "plainText") {
        return Err(bad("trigger.ownChannels must contain plainText"));
    }
    let mut uniq = t.own_channels.clone();
    uniq.sort_unstable();
    uniq.dedup();
    if uniq.len() != t.own_channels.len() {
        return Err(bad("trigger.ownChannels must be unique"));
    }
    if !(0.0..=1.0).contains(&t.min_confidence) {
        return Err(bad("trigger.minConfidence must be in [0, 1]"));
    }

    // processor
    let p = &r.processor;
    if !ENGINES.contains(&p.engine.as_str()) {
        return Err(bad("invalid processor.engine"));
    }
    if p.steps.len() > MAX_STEPS {
        return Err(bad("processor.steps must have <= 16 items"));
    }
    if p.engine == "regex_pipeline" && p.steps.is_empty() {
        return Err(bad("regex_pipeline requires >= 1 step"));
    }
    for step in &p.steps {
        if step.pattern.is_empty() || step.pattern.chars().count() > MAX_PATTERN_LEN {
            return Err(bad("step.pattern must be 1..=512 chars"));
        }
        if step.replacement.chars().count() > MAX_REPLACEMENT_LEN {
            return Err(bad("step.replacement must be <= 1024 chars"));
        }
        let flags_re = Regex::new(r"^[imsU]*$").expect("static regex");
        if !flags_re.is_match(&step.flags) {
            return Err(bad("step.flags must match ^[imsU]*$"));
        }
        // 正则预编译验证（TDD §7.4: 拒绝无法编译的正则，防止回调期 panic）
        if Regex::new(&step.pattern).is_err() {
            return Err(bad("step.pattern failed to compile"));
        }
    }
    if let Some(prompt) = &p.fallback_prompt {
        if prompt.chars().count() > MAX_FALLBACK_PROMPT_LEN {
            return Err(bad("fallbackPrompt must be <= 4096 chars"));
        }
    }
    if p.provider_timeout_ms < 1 || p.provider_timeout_ms > MAX_PROVIDER_TIMEOUT_MS {
        return Err(bad("providerTimeoutMs must be in 1..=8"));
    }

    // output
    let o = &r.output;
    if !POLICIES.contains(&o.plain_text_policy.as_str()) {
        return Err(bad("invalid output.plainTextPolicy"));
    }
    if o.channels.plain_text.default_handler.is_empty() {
        return Err(bad("output.channels.plainText.defaultHandler is required"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 合法 llm_stream 配方 JSON（对齐 IPC Spec §5 示例结构）
    fn valid_llm_json() -> String {
        r#"{
            "$schema": "https://raw.githubusercontent.com/PasteFlowApp/pasteflow/main/schema/pipe-v1.json",
            "id": "community.meeting_summary",
            "name": "会议纪要汇总",
            "description": "把复制片段汇总为会议纪要",
            "author": "tester",
            "version": "1.0.0",
            "icon": "sparkles",
            "category": "ai_writing",
            "tier": "L3_STREAMING_AI",
            "trigger": {
                "mode": "MANUAL_ONLY",
                "matchContentTags": ["markdown"],
                "matchSourceApps": ["*"],
                "ownChannels": ["plainText"],
                "minConfidence": 0.5
            },
            "processor": {
                "engine": "llm_stream",
                "handlerName": "llm_summary",
                "fallbackPrompt": "请汇总以下文本",
                "providerTimeoutMs": 8
            },
            "output": {
                "channels": {
                    "plainText": { "defaultHandler": "plain_passthrough" }
                },
                "plainTextPolicy": "CLEANED",
                "preserveOriginal": true
            }
        }"#
        .to_string()
    }

    fn valid_regex_json() -> String {
        valid_llm_json()
            .replace("community.meeting_summary", "community.link_clean")
            .replace("llm_stream", "regex_pipeline")
            .replace(
                r#""handlerName": "llm_summary","#,
                r#""handlerName": "link_clean", "steps": [{"pattern": "[?&]utm_[a-z]+=[^&]+", "replacement": ""}],"#,
            )
    }

    #[test]
    fn valid_llm_recipe_requires_confirmation() {
        // 未确认 -> PROMPT_REVIEW_REQUIRED
        let err = import_recipe(&valid_llm_json(), false).unwrap_err();
        assert_eq!(err, ImportError::PromptReviewRequired);
        assert_eq!(err.code(), "PROMPT_REVIEW_REQUIRED");
        // 确认后导入成功
        assert!(import_recipe(&valid_llm_json(), true).is_ok());
    }

    #[test]
    fn valid_regex_recipe_imports_without_review() {
        let r = import_recipe(&valid_regex_json(), false).unwrap();
        assert_eq!(r.processor.engine, "regex_pipeline");
    }

    #[test]
    fn unknown_fields_rejected() {
        let json = valid_llm_json().replace("\"name\"", "\"hackerField\": 1, \"name\"");
        let err = import_recipe(&json, true).unwrap_err();
        assert_eq!(err.code(), "RECIPE_SCHEMA_INVALID");
    }

    #[test]
    fn provider_timeout_over_8_rejected() {
        let json = valid_llm_json().replace("\"providerTimeoutMs\": 8", "\"providerTimeoutMs\": 9");
        let err = import_recipe(&json, true).unwrap_err();
        assert!(matches!(err, ImportError::SchemaInvalid(_)));
    }

    #[test]
    fn provider_timeout_zero_rejected() {
        let json = valid_llm_json().replace("\"providerTimeoutMs\": 8", "\"providerTimeoutMs\": 0");
        assert!(import_recipe(&json, true).is_err());
    }

    #[test]
    fn invalid_regex_pattern_rejected() {
        let json = valid_regex_json().replace("[?&]utm_[a-z]+=[^&]+", "[unclosed(");
        let err = import_recipe(&json, false).unwrap_err();
        assert_eq!(err.code(), "RECIPE_SCHEMA_INVALID");
    }

    #[test]
    fn invalid_step_flags_rejected() {
        let json = valid_regex_json().replace("\"replacement\": \"\"", "\"replacement\": \"\", \"flags\": \"x\"");
        assert!(import_recipe(&json, false).is_err());
    }

    #[test]
    fn too_many_steps_rejected() {
        let steps: Vec<String> = (0..17).map(|i| format!(r#"{{"pattern": "a{i}*", "replacement": ""}}"#)).collect();
        let json = valid_regex_json().replace(
            r#"[{"pattern": "[?&]utm_[a-z]+=[^&]+", "replacement": ""}]"#,
            &format!("[{}]", steps.join(",")),
        );
        assert!(import_recipe(&json, false).is_err());
    }

    #[test]
    fn plain_text_channel_required() {
        let json = valid_llm_json().replace(
            r#""plainText": { "defaultHandler": "plain_passthrough" }"#,
            r#""plainText": { "defaultHandler": "" }"#,
        );
        assert!(import_recipe(&json, true).is_err());
    }

    #[test]
    fn own_channels_must_contain_plain_text() {
        let json = valid_llm_json().replace(r#"["plainText"]"#, r#"["html"]"#);
        let err = import_recipe(&json, true).unwrap_err();
        assert!(matches!(err, ImportError::SchemaInvalid(_)));
    }

    #[test]
    fn duplicate_own_channels_rejected() {
        let json = valid_llm_json().replace(r#"["plainText"]"#, r#"["plainText", "plainText"]"#);
        assert!(import_recipe(&json, true).is_err());
    }

    #[test]
    fn invalid_category_and_tier_rejected() {
        let json = valid_llm_json().replace("\"category\": \"ai_writing\"", "\"category\": \"malware\"");
        assert!(import_recipe(&json, true).is_err());
        let json = valid_llm_json().replace("\"tier\": \"L3_STREAMING_AI\"", "\"tier\": \"L9\"");
        assert!(import_recipe(&json, true).is_err());
    }

    #[test]
    fn bad_id_and_version_rejected() {
        let json = valid_llm_json().replace("community.meeting_summary", "Bad ID!");
        assert!(import_recipe(&json, true).is_err());
        let json = valid_llm_json().replace("\"version\": \"1.0.0\"", "\"version\": \"1.0\"");
        assert!(import_recipe(&json, true).is_err());
    }

    #[test]
    fn min_confidence_bounds_enforced() {
        let json = valid_llm_json().replace("\"minConfidence\": 0.5", "\"minConfidence\": 1.5");
        assert!(import_recipe(&json, true).is_err());
    }

    #[test]
    fn malformed_json_rejected() {
        let err = import_recipe("{not json", true).unwrap_err();
        assert_eq!(err.code(), "RECIPE_SCHEMA_INVALID");
    }

    #[test]
    fn fallback_prompt_length_enforced() {
        let long = "a".repeat(4097);
        let json = valid_llm_json().replace("请汇总以下文本", &long);
        assert!(import_recipe(&json, true).is_err());
    }
}
