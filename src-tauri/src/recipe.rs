//! P1-12 PipeRecipe — TDD §3.4 / IPC Spec §4 (PipeRecipe JSON Schema)
//!
//! 内置 PipeRecipe (v0.1.0 仅两个 L1 杀手滤镜):
//!   1. markdown-rtf  : Markdown -> 富文本 (HTML/RTF/纯文本三通道)
//!   2. social-clean  : 社媒链接净化 (追踪参数剥离, 仅纯文本)
//!
//! Recipe 为声明式 JSON, Schema 校验在加载时执行 (非法 recipe 拒绝入库)。
//! v0.1.5 P2 滤镜商店将支持用户自定义 recipe, 本模块是其地基。
//!
//! 铁律: L1 算子必须确定性 + <5ms @64KB (TDD §7.4); recipe 不可声明网络算子。

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// PipeRecipe Schema 版本 (IPC Spec §4)
pub const RECIPE_SCHEMA_VERSION: u32 = 1;

/// 算子名称 (与 filters 模块一一对应)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpKind {
    /// Markdown -> HTML 富文本
    MarkdownToHtml,
    /// Markdown -> RTF
    MarkdownToRtf,
    /// Markdown -> IM 纯文本净化
    MarkdownToImText,
    /// 社媒追踪参数剥离
    SocialLinkClean,
    /// DLP 静默三不检测 (检测分流, 绝不改写明文)
    DlpSilentGuard,
}

impl OpKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            OpKind::MarkdownToHtml => "markdown_to_html",
            OpKind::MarkdownToRtf => "markdown_to_rtf",
            OpKind::MarkdownToImText => "markdown_to_im_text",
            OpKind::SocialLinkClean => "social_link_clean",
            OpKind::DlpSilentGuard => "dlp_silent_guard",
        }
    }

    /// 算子是否确定性本地规则 (recipe 约束: 禁网络算子)
    pub fn is_deterministic_local(&self) -> bool {
        true // v0.1.0 全部 L1 算子均为确定性本地规则
    }
}

/// Recipe 输出通道
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputChannel {
    Html,
    Rtf,
    Plain,
}

/// 单条 recipe 定义
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipeRecipe {
    /// Schema 版本
    pub schema_version: u32,
    /// 唯一 ID (kebab-case, 供热键/IPC 引用)
    pub id: String,
    /// 显示名 (菜单栏/滤镜商店)
    pub display_name: String,
    /// 管道算子序列 (顺序执行)
    pub ops: Vec<OpKind>,
    /// 输出通道声明 (至少一个)
    pub outputs: Vec<OutputChannel>,
    /// 触发目标应用 bundle 空集 = 全局生效; 非空 = 白名单
    pub target_bundles: Vec<String>,
    /// 最大输入体积 (bytes, 超出则拒绝接管; 对齐 G5 分级)
    pub max_input_bytes: usize,
}

/// Schema 校验错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecipeError {
    /// schema_version 不匹配
    UnsupportedVersion(u32),
    /// id 为空或格式非法 (需 kebab-case)
    InvalidId,
    /// id 重复
    DuplicateId(String),
    /// ops 为空
    EmptyOps,
    /// outputs 为空
    EmptyOutputs,
    /// max_input_bytes 为 0 或超过 G5 上限
    InvalidMaxBytes,
}

impl std::fmt::Display for RecipeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecipeError::UnsupportedVersion(v) => write!(f, "unsupported schema_version: {v}"),
            RecipeError::InvalidId => write!(f, "invalid recipe id (must be non-empty kebab-case)"),
            RecipeError::DuplicateId(id) => write!(f, "duplicate recipe id: {id}"),
            RecipeError::EmptyOps => write!(f, "ops must not be empty"),
            RecipeError::EmptyOutputs => write!(f, "outputs must not be empty"),
            RecipeError::InvalidMaxBytes => write!(f, "max_input_bytes must be in (0, 256KiB]"),
        }
    }
}

impl std::error::Error for RecipeError {}

/// G5 分级上限 (对齐 format_gate STAGE2_REFUSE_MIN_BYTES)
const G5_MAX_BYTES: usize = 256 * 1024;

/// 校验单条 recipe (IPC Spec §4 Schema)
pub fn validate_recipe(r: &PipeRecipe) -> Result<(), RecipeError> {
    if r.schema_version != RECIPE_SCHEMA_VERSION {
        return Err(RecipeError::UnsupportedVersion(r.schema_version));
    }
    // id: 非空 + kebab-case (小写字母数字 + 连字符)
    let id_ok = !r.id.is_empty()
        && r.id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !r.id.starts_with('-')
        && !r.id.ends_with('-');
    if !id_ok {
        return Err(RecipeError::InvalidId);
    }
    if r.ops.is_empty() {
        return Err(RecipeError::EmptyOps);
    }
    // 算子必须确定性本地 (禁网络算子, TDD §7.4)
    if !r.ops.iter().all(|o| o.is_deterministic_local()) {
        return Err(RecipeError::EmptyOps); // 不可达, 语义保留
    }
    if r.outputs.is_empty() {
        return Err(RecipeError::EmptyOutputs);
    }
    if r.max_input_bytes == 0 || r.max_input_bytes > G5_MAX_BYTES {
        return Err(RecipeError::InvalidMaxBytes);
    }
    Ok(())
}

/// Recipe 集合 (滤镜商店地基)
pub struct RecipeStore {
    recipes: Vec<PipeRecipe>,
    ids: HashSet<String>,
}

impl Default for RecipeStore {
    fn default() -> Self {
        Self::new()
    }
}

impl RecipeStore {
    pub fn new() -> Self {
        Self {
            recipes: Vec::new(),
            ids: HashSet::new(),
        }
    }

    /// 校验并注册; 失败返回错误 (不入集合)
    pub fn register(&mut self, r: PipeRecipe) -> Result<(), RecipeError> {
        validate_recipe(&r)?;
        if self.ids.contains(&r.id) {
            return Err(RecipeError::DuplicateId(r.id.clone()));
        }
        self.ids.insert(r.id.clone());
        self.recipes.push(r);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&PipeRecipe> {
        self.recipes.iter().find(|r| r.id == id)
    }

    pub fn all(&self) -> &[PipeRecipe] {
        &self.recipes
    }

    pub fn len(&self) -> usize {
        self.recipes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.recipes.is_empty()
    }

    /// 加载内置 v0.1.0 recipe 集 (两个 L1 杀手滤镜)
    pub fn with_builtin() -> Result<Self, RecipeError> {
        let mut store = Self::new();
        store.register(builtin_markdown_rtf())?;
        store.register(builtin_social_clean())?;
        Ok(store)
    }
}

/// 内置 Recipe 1: Markdown -> 富文本 (Master Spec §3.3 Filter A / D1 杀手滤镜)
pub fn builtin_markdown_rtf() -> PipeRecipe {
    PipeRecipe {
        schema_version: RECIPE_SCHEMA_VERSION,
        id: "markdown-rtf".into(),
        display_name: "Markdown → 富文本".into(),
        ops: vec![
            OpKind::MarkdownToHtml,
            OpKind::MarkdownToRtf,
            OpKind::MarkdownToImText,
            OpKind::DlpSilentGuard,
        ],
        outputs: vec![OutputChannel::Html, OutputChannel::Rtf, OutputChannel::Plain],
        target_bundles: vec![], // 全局生效 (路由由 PipeExecutor 目标启发式决定)
        max_input_bytes: G5_MAX_BYTES,
    }
}

/// 内置 Recipe 2: 社媒链接净化 (Master Spec §3.3 Filter B)
pub fn builtin_social_clean() -> PipeRecipe {
    PipeRecipe {
        schema_version: RECIPE_SCHEMA_VERSION,
        id: "social-clean".into(),
        display_name: "社媒链接净化".into(),
        ops: vec![OpKind::SocialLinkClean, OpKind::DlpSilentGuard],
        outputs: vec![OutputChannel::Plain],
        target_bundles: vec![],
        max_input_bytes: G5_MAX_BYTES,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_recipes_valid() {
        assert!(validate_recipe(&builtin_markdown_rtf()).is_ok());
        assert!(validate_recipe(&builtin_social_clean()).is_ok());
    }

    #[test]
    fn builtin_store_loads_both() {
        let store = RecipeStore::with_builtin().unwrap();
        assert_eq!(store.len(), 2);
        assert!(store.get("markdown-rtf").is_some());
        assert!(store.get("social-clean").is_some());
    }

    #[test]
    fn rejects_wrong_schema_version() {
        let mut r = builtin_markdown_rtf();
        r.schema_version = 2;
        assert_eq!(validate_recipe(&r), Err(RecipeError::UnsupportedVersion(2)));
    }

    #[test]
    fn rejects_invalid_id() {
        let mut r = builtin_markdown_rtf();
        r.id = "Bad_ID!".into();
        assert_eq!(validate_recipe(&r), Err(RecipeError::InvalidId));
        r.id = "-lead".into();
        assert_eq!(validate_recipe(&r), Err(RecipeError::InvalidId));
        r.id = String::new();
        assert_eq!(validate_recipe(&r), Err(RecipeError::InvalidId));
    }

    #[test]
    fn rejects_empty_ops_or_outputs() {
        let mut r = builtin_markdown_rtf();
        r.ops.clear();
        assert_eq!(validate_recipe(&r), Err(RecipeError::EmptyOps));
        let mut r2 = builtin_social_clean();
        r2.outputs.clear();
        assert_eq!(validate_recipe(&r2), Err(RecipeError::EmptyOutputs));
    }

    #[test]
    fn rejects_invalid_max_bytes() {
        let mut r = builtin_markdown_rtf();
        r.max_input_bytes = 0;
        assert_eq!(validate_recipe(&r), Err(RecipeError::InvalidMaxBytes));
        r.max_input_bytes = 512 * 1024; // 超 G5 上限
        assert_eq!(validate_recipe(&r), Err(RecipeError::InvalidMaxBytes));
    }

    #[test]
    fn rejects_duplicate_id() {
        let mut store = RecipeStore::new();
        store.register(builtin_markdown_rtf()).unwrap();
        let dup = store.register(builtin_markdown_rtf());
        assert!(matches!(dup, Err(RecipeError::DuplicateId(_))));
    }

    #[test]
    fn recipe_serializes_roundtrip() {
        // IPC Spec §4: PipeRecipe JSON Schema 往返稳定
        let r = builtin_social_clean();
        let json = serde_json::to_string(&r).unwrap();
        let back: PipeRecipe = serde_json::from_str(&json).unwrap();
        assert_eq!(r, back);
    }

    #[test]
    fn markdown_recipe_three_channels() {
        let r = builtin_markdown_rtf();
        assert_eq!(r.outputs.len(), 3);
        assert!(r.outputs.contains(&OutputChannel::Html));
        assert!(r.outputs.contains(&OutputChannel::Rtf));
        assert!(r.outputs.contains(&OutputChannel::Plain));
    }

    #[test]
    fn opkind_string_names_stable() {
        assert_eq!(OpKind::MarkdownToHtml.as_str(), "markdown_to_html");
        assert_eq!(OpKind::DlpSilentGuard.as_str(), "dlp_silent_guard");
    }
}
