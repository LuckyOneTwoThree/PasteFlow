//! P2-8 Tauri IPC 命令层 (IPC Spec v1.0 §3 Commands / §4 Events)
//!
//! 把 P2 已建模块（storage / stack / drawer / recipe_import）串成可被 Webview 调用的
//! 命令与事件契约。命令为薄包装，核心逻辑落在 `ipc_logic` 自由函数上以便单元测试。
//!
//! 错误码（IPC Spec §3.3）以 `IpcError` 表达，前端据 `code` 分流。

use crate::drawer::DrawerViewModel;
use crate::recipe::RecipeStore;
use crate::recipe_import::{import_recipe as import_recipe_strict, CommunityRecipe, ImportError};
use crate::stack::{ConsumeMode, ConsumeResult, StackQueue, SummaryPromptType};
use crate::storage::{ClipboardItem, HistoryDb};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

// —— IPC 错误码 (IPC Spec §3.3) ——

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcError {
    pub code: String,
    pub message: String,
}

impl IpcError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into() }
    }
}

pub type IpcResult<T> = Result<T, IpcError>;

// —— DTO (IPC Spec §3) ——

/// ClipboardItemDto
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_snake_case)] // IPC Spec §3 DTO 字段为 camelCase
pub struct ClipboardItemDto {
    pub id: String,
    pub createdAt: i64,
    pub contentHash: String,
    pub primaryType: String,
    pub plainTextExcerpt: Option<String>,
    pub sourceAppName: String,
    pub sourceBundleId: Option<String>,
    pub tags: Vec<String>,
    pub plainTextPolicy: String,
    pub isPinned: bool,
    pub inStack: bool,
}

impl From<&ClipboardItem> for ClipboardItemDto {
    fn from(it: &ClipboardItem) -> Self {
        Self {
            id: it.id.clone(),
            createdAt: it.created_at,
            contentHash: it.content_hash.clone(),
            primaryType: it.primary_type.clone(),
            plainTextExcerpt: it.raw_plain_text.as_deref().map(excerpt_240),
            sourceAppName: it.source_app_name.clone(),
            sourceBundleId: it.source_bundle_id.clone(),
            tags: parse_tags(&it.tags_json),
            plainTextPolicy: it.plain_text_policy.clone(),
            isPinned: it.is_pinned,
            inStack: it.in_stack,
        }
    }
}

fn excerpt_240(s: &str) -> String {
    s.chars().take(240).collect()
}

fn parse_tags(json: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(json).unwrap_or_default()
}

#[derive(Debug, Deserialize)]
pub struct SearchHistoryArgs {
    pub query: String,
    #[serde(default)]
    pub tagFilter: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    50
}

#[derive(Debug, Serialize)]
pub struct SearchHistoryReturn {
    pub items: Vec<ClipboardItemDto>,
    pub total: i64,
}

#[derive(Debug, Deserialize)]
pub struct TogglePinArgs {
    pub itemId: String,
    pub pinned: bool,
}

#[derive(Debug, Serialize)]
pub struct TogglePinReturn {
    pub success: bool,
}

#[derive(Debug, Deserialize)]
pub struct SetStackModeArgs {
    pub active: bool,
}

#[derive(Debug, Serialize)]
pub struct SetStackModeReturn {
    pub active: bool,
    pub count: usize,
}

#[derive(Debug, Deserialize)]
pub struct ConsumeStackArgs {
    pub mode: ConsumeMode,
    #[serde(default)]
    pub summaryPromptType: Option<SummaryPromptType>,
}

#[derive(Debug, Serialize)]
pub struct ConsumeStackReturn {
    pub consumedCount: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub streamId: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ImportRecipeArgs {
    pub rawJson: String,
    pub userConfirmedPromptReview: bool,
}

#[derive(Debug, Serialize)]
pub struct ImportRecipeReturn {
    pub importedPipeId: String,
}

// —— IPC 状态 (由 lib.rs manage) ——

pub struct IpcState {
    pub history: Mutex<HistoryDb>,
    pub stack: Mutex<StackQueue>,
    pub drawer: Mutex<DrawerViewModel>,
    pub imported_recipes: Mutex<Vec<CommunityRecipe>>,
    pub builtin_recipes: RecipeStore,
}

impl IpcState {
    pub fn new(history: HistoryDb, builtin: RecipeStore) -> Self {
        Self {
            history: Mutex::new(history),
            stack: Mutex::new(StackQueue::new()),
            drawer: Mutex::new(DrawerViewModel::new()),
            imported_recipes: Mutex::new(Vec::new()),
            builtin_recipes: builtin,
        }
    }
}

// —— 核心逻辑 (自由函数, 单测覆盖) ——

/// 检索历史：`HistoryDb::search`（≥3 字符 FTS5 trigram / 1–2 字符 LIKE 回退）
/// + 类型过滤 + offset 分页
pub fn search_history(db: &HistoryDb, vm: &DrawerViewModel, args: &SearchHistoryArgs) -> IpcResult<SearchHistoryReturn> {
    // 全量命中检索（历史上限 500 条），保证 total 为过滤后真实命中数；分页在内存切片完成
    let results = db
        .search(&args.query, HISTORY_MAX_ITEMS)
        .map_err(|e| IpcError::new("STORAGE_ERROR", e.to_string()))?;
    let filtered = vm.apply(results);
    let total = filtered.len() as i64;
    let start = args.offset.max(0) as usize;
    let items = filtered
        .into_iter()
        .skip(start)
        .take(args.limit.max(0) as usize)
        .map(|it| ClipboardItemDto::from(&it))
        .collect();
    Ok(SearchHistoryReturn { items, total })
}

/// 历史库条目上限 (Master Spec §11: 抽屉默认展示最近 500 条)
const HISTORY_MAX_ITEMS: i64 = 500;

/// 置顶/取消置顶
pub fn toggle_pin(db: &HistoryDb, args: &TogglePinArgs) -> IpcResult<TogglePinReturn> {
    db.set_pinned(&args.itemId, args.pinned)
        .map_err(|e| IpcError::new("STORAGE_ERROR", e.to_string()))?;
    Ok(TogglePinReturn { success: true })
}

/// 开关贪吃蛇收集槽；关闭时队列清空 (IPC Spec `cmd_set_stack_mode`)
pub fn set_stack_mode(stack: &mut StackQueue, active: bool) -> SetStackModeReturn {
    stack.set_active(active);
    SetStackModeReturn { active: stack.is_active(), count: stack.len() }
}

/// 一键消费贪吃蛇队列 (IPC Spec `cmd_consume_stack_queue`)
/// 前两种模式同步合并写入剪贴板（写入由调用方/命令层负责），此函数返回合并结果并清空队列。
pub fn consume_stack(stack: &mut StackQueue, args: &ConsumeStackArgs) -> IpcResult<ConsumeStackReturn> {
    if stack.is_empty() {
        return Err(IpcError::new("STACK_EMPTY", "stack queue is empty"));
    }
    let r: ConsumeResult = stack.consume(args.mode, args.summaryPromptType);
    Ok(ConsumeStackReturn {
        consumedCount: r.consumed_count,
        streamId: r.stream_id,
    })
}

/// 社区配方导入（严格校验 + llm_stream 审查确认）
pub fn import_recipe(raw_json: &str, user_confirmed: bool) -> IpcResult<CommunityRecipe> {
    import_recipe_strict(raw_json, user_confirmed).map_err(|e| match e {
        ImportError::SchemaInvalid(msg) => IpcError::new("RECIPE_SCHEMA_INVALID", msg),
        ImportError::PromptReviewRequired => IpcError::new(
            "PROMPT_REVIEW_REQUIRED",
            "llm_stream recipe requires prompt review confirmation",
        ),
    })
}

// —— Tauri Commands (薄包装, 转发到核心逻辑) ——

#[tauri::command]
pub fn cmd_search_history(
    state: tauri::State<'_, IpcState>,
    args: SearchHistoryArgs,
) -> IpcResult<SearchHistoryReturn> {
    let db = state.history.lock().unwrap();
    let vm = state.drawer.lock().unwrap();
    search_history(&db, &vm, &args)
}

#[tauri::command]
pub fn cmd_toggle_pin_item(
    state: tauri::State<'_, IpcState>,
    args: TogglePinArgs,
) -> IpcResult<TogglePinReturn> {
    let db = state.history.lock().unwrap();
    toggle_pin(&db, &args)
}

#[tauri::command]
pub fn cmd_set_stack_mode(
    state: tauri::State<'_, IpcState>,
    args: SetStackModeArgs,
) -> IpcResult<SetStackModeReturn> {
    let mut stack = state.stack.lock().unwrap();
    Ok(set_stack_mode(&mut stack, args.active))
}

#[tauri::command]
pub fn cmd_consume_stack_queue(
    state: tauri::State<'_, IpcState>,
    args: ConsumeStackArgs,
) -> IpcResult<ConsumeStackReturn> {
    let mut stack = state.stack.lock().unwrap();
    consume_stack(&mut stack, &args)
}

#[tauri::command]
pub fn cmd_import_pipe_recipe(
    state: tauri::State<'_, IpcState>,
    args: ImportRecipeArgs,
) -> IpcResult<ImportRecipeReturn> {
    let recipe = import_recipe(&args.rawJson, args.userConfirmedPromptReview)?;
    let imported_id = recipe.id.clone();
    state.imported_recipes.lock().unwrap().push(recipe);
    Ok(ImportRecipeReturn { importedPipeId: imported_id })
}

#[tauri::command]
pub fn cmd_list_pipe_recipes(
    state: tauri::State<'_, IpcState>,
) -> IpcResult<Vec<serde_json::Value>> {
    let mut out: Vec<serde_json::Value> = state
        .builtin_recipes
        .all()
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "displayName": r.display_name,
                "mode": "AUTO",
                "plainTextPolicy": "DYNAMIC_BY_TARGET",
                "builtin": true
            })
        })
        .collect();
    for r in state.imported_recipes.lock().unwrap().iter() {
        out.push(serde_json::json!({
            "id": r.id,
            "displayName": r.name,
            "mode": "MANUAL",
            "plainTextPolicy": r.output.plain_text_policy,
            "builtin": false
        }));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stack::StackEntry;
    use crate::storage::make_item;

    fn db_with(items: &[(&str, &str)]) -> HistoryDb {
        let db = HistoryDb::in_memory().unwrap();
        for (text, app) in items {
            db.insert_item(&make_item(text, app, None)).unwrap();
        }
        db
    }

    #[test]
    fn search_history_returns_items_and_total() {
        let db = db_with(&[("架构周报总结", "Notion"), ("other content", "Notes")]);
        let vm = DrawerViewModel::new();
        let args = SearchHistoryArgs { query: "周报".into(), tagFilter: None, limit: 10, offset: 0 };
        let r = search_history(&db, &vm, &args).unwrap();
        assert_eq!(r.total, 1);
        assert_eq!(r.items.len(), 1);
        assert_eq!(r.items[0].sourceAppName, "Notion");
    }

    #[test]
    fn search_history_offset_paginates() {
        let db = db_with(&[("alpha", "A"), ("beta", "B"), ("gamma", "C")]);
        let vm = DrawerViewModel::new();
        let args = SearchHistoryArgs { query: String::new(), tagFilter: None, limit: 1, offset: 1 };
        let r = search_history(&db, &vm, &args).unwrap();
        assert_eq!(r.total, 3);
        assert_eq!(r.items.len(), 1);
        assert_eq!(r.items[0].id.len() > 0, true);
    }

    #[test]
    fn toggle_pin_marks_item() {
        let db = db_with(&[("pin me", "X")]);
        let item = db.recent(1).unwrap().into_iter().next().unwrap();
        let r = toggle_pin(&db, &TogglePinArgs { itemId: item.id.clone(), pinned: true }).unwrap();
        assert!(r.success);
        assert!(db.find_by_hash(&item.content_hash).unwrap().unwrap().is_pinned);
    }

    #[test]
    fn set_stack_mode_toggles_and_clears() {
        let mut stack = StackQueue::new();
        let r = set_stack_mode(&mut stack, true);
        assert!(r.active);
        assert_eq!(r.count, 0);
        stack.push(StackEntry { id: "1".into(), plain_text: "a".into(), source_app_name: "X".into() });
        let r = set_stack_mode(&mut stack, false);
        assert!(!r.active);
        assert_eq!(r.count, 0); // 关闭清空
    }

    #[test]
    fn consume_stack_join_newline_writes_merged() {
        let mut stack = StackQueue::new();
        stack.set_active(true);
        stack.push(StackEntry { id: "1".into(), plain_text: "hi".into(), source_app_name: "X".into() });
        stack.push(StackEntry { id: "2".into(), plain_text: "there".into(), source_app_name: "X".into() });
        let r = consume_stack(&mut stack, &ConsumeStackArgs {
            mode: ConsumeMode::JoinNewline,
            summaryPromptType: None,
        })
        .unwrap();
        assert_eq!(r.consumedCount, 2);
        assert!(stack.is_empty());
    }

    #[test]
    fn consume_empty_stack_errors() {
        let mut stack = StackQueue::new();
        let err = consume_stack(&mut stack, &ConsumeStackArgs {
            mode: ConsumeMode::JoinNewline,
            summaryPromptType: None,
        })
        .unwrap_err();
        assert_eq!(err.code, "STACK_EMPTY");
    }

    #[test]
    fn import_recipe_maps_error_codes() {
        // llm_stream 未确认 -> PROMPT_REVIEW_REQUIRED
        let llm = r#"{
            "$schema": "https://x/schema.json", "id": "community.a", "name": "n12",
            "description": "desc", "author": "a", "version": "1.0.0", "icon": "i",
            "category": "ai_writing", "tier": "L3_STREAMING_AI",
            "trigger": { "mode": "MANUAL_ONLY", "matchContentTags": ["markdown"],
                "matchSourceApps": ["*"], "ownChannels": ["plainText"], "minConfidence": 0.5 },
            "processor": { "engine": "llm_stream", "handlerName": "h",
                "fallbackPrompt": "p", "providerTimeoutMs": 8 },
            "output": { "channels": { "plainText": { "defaultHandler": "d" } },
                "plainTextPolicy": "CLEANED", "preserveOriginal": true }
        }"#;
        assert_eq!(import_recipe(llm, false).unwrap_err().code, "PROMPT_REVIEW_REQUIRED");
        assert!(import_recipe(llm, true).is_ok());
        // 非法 JSON -> RECIPE_SCHEMA_INVALID
        assert_eq!(import_recipe("{bad", true).unwrap_err().code, "RECIPE_SCHEMA_INVALID");
    }

    #[test]
    fn item_dto_truncates_excerpt_and_parses_tags() {
        let mut it = make_item(&"x".repeat(500), "App", None);
        it.tags_json = r#"["markdown", "code"]"#.into();
        let dto = ClipboardItemDto::from(&it);
        assert_eq!(dto.plainTextExcerpt.unwrap().chars().count(), 240);
        assert_eq!(dto.tags, vec!["markdown".to_string(), "code".to_string()]);
    }
}
