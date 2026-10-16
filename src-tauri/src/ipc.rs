//! P2-8 Tauri IPC 命令层 (IPC Spec v1.0 §3 Commands / §4 Events)
//!
//! 把 P2 已建模块（storage / stack / drawer / recipe_import）串成可被 Webview 调用的
//! 命令与事件契约。命令为薄包装，核心逻辑落在 `ipc_logic` 自由函数上以便单元测试。
//!
//! 错误码（IPC Spec §3.3）以 `IpcError` 表达，前端据 `code` 分流。

#![allow(non_snake_case)] // IPC Spec §3 DTO 字段为 camelCase (跨进程契约)

use crate::drawer::DrawerViewModel;
use crate::recipe::RecipeStore;
use crate::recipe_import::{import_recipe as import_recipe_strict, CommunityRecipe, ImportError};
use crate::stack::{ConsumeMode, ConsumeResult, StackQueue, SummaryPromptType};
use crate::storage::{ClipboardItem, HistoryDb};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

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

/// StackEntryDto (IPC Spec §4 `pasteflow://stack-updated` items)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackEntryDto {
    pub id: String,
    pub plain_text: String,
    pub source_app_name: String,
    pub index: usize,
}

impl From<(usize, &crate::stack::StackEntry)> for StackEntryDto {
    fn from((index, e): (usize, &crate::stack::StackEntry)) -> Self {
        Self {
            id: e.id.clone(),
            plain_text: e.plain_text.clone(),
            source_app_name: e.source_app_name.clone(),
            index,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ListStackReturn {
    pub active: bool,
    pub items: Vec<StackEntryDto>,
}

#[derive(Debug, Deserialize)]
pub struct StackItemArgs {
    pub index: usize,
}

#[derive(Debug, Deserialize)]
pub struct StackReorderArgs {
    pub from: usize,
    pub to: usize,
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
    /// JOIN_NEWLINE / DEDUP_NUMBERED 合并结果（命令层据此原子写剪贴板）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mergedText: Option<String>,
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
    /// 共享历史库 (引擎复制事件也写入, 抽屉检索同一份数据)
    pub history: Arc<Mutex<HistoryDb>>,
    /// 共享贪吃蛇队列 (引擎复制事件入队, 面板消费)
    pub stack: Arc<Mutex<StackQueue>>,
    pub drawer: Mutex<DrawerViewModel>,
    pub imported_recipes: Mutex<Vec<CommunityRecipe>>,
    pub builtin_recipes: RecipeStore,
}

impl IpcState {
    pub fn new(
        history: Arc<Mutex<HistoryDb>>,
        stack: Arc<Mutex<StackQueue>>,
        builtin: RecipeStore,
    ) -> Self {
        Self {
            history,
            stack,
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
    let filter = args
        .tagFilter
        .as_deref()
        .and_then(|t| match t.to_uppercase().as_str() {
            "MARKDOWN" => Some(crate::drawer::TypeFilter::Markdown),
            "LINK" => Some(crate::drawer::TypeFilter::Link),
            "CODE" => Some(crate::drawer::TypeFilter::Code),
            "IMAGE" => Some(crate::drawer::TypeFilter::Image),
            "ALL" | "" => Some(crate::drawer::TypeFilter::All),
            _ => None,
        })
        .unwrap_or(vm.filter);
    let filtered: Vec<_> = results.into_iter().filter(|it| filter.matches(it)).collect();
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
/// 前两种模式同步合并文本（写剪贴板由命令层负责），此函数返回合并结果并清空队列。
pub fn consume_stack(stack: &mut StackQueue, args: &ConsumeStackArgs) -> IpcResult<ConsumeStackReturn> {
    if stack.is_empty() {
        return Err(IpcError::new("STACK_EMPTY", "stack queue is empty"));
    }
    let r: ConsumeResult = stack.consume(args.mode, args.summaryPromptType);
    Ok(ConsumeStackReturn {
        consumedCount: r.consumed_count,
        streamId: r.stream_id,
        mergedText: r.merged_text,
    })
}

/// 贪吃蛇队列快照 (UI Spec §4.2 面板列表)
pub fn list_stack(stack: &StackQueue) -> ListStackReturn {
    ListStackReturn {
        active: stack.is_active(),
        items: stack
            .entries()
            .iter()
            .enumerate()
            .map(StackEntryDto::from)
            .collect(),
    }
}

/// 单条移除 (UI Spec §4.2)
pub fn remove_stack_item(stack: &mut StackQueue, index: usize) -> IpcResult<ListStackReturn> {
    if stack.remove_at(index).is_none() {
        return Err(IpcError::new("STACK_ITEM_NOT_FOUND", "index out of range"));
    }
    Ok(list_stack(stack))
}

/// 拖拽排序 (UI Spec §4.2)
pub fn reorder_stack_items(stack: &mut StackQueue, from: usize, to: usize) -> IpcResult<ListStackReturn> {
    if !stack.reorder(from, to) {
        return Err(IpcError::new("STACK_ITEM_NOT_FOUND", "index out of range"));
    }
    Ok(list_stack(stack))
}

/// 贪吃蛇队列文本拼接（L3 汇总输入源, 片段间空行分隔）
pub fn stack_source_text(stack: &StackQueue) -> String {
    stack
        .entries()
        .iter()
        .map(|e| e.plain_text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// 汇总提示词类型 → L3 提示词（默认会议纪要）
pub fn summary_to_prompt_type(p: Option<SummaryPromptType>) -> crate::l3::PromptType {
    match p.unwrap_or(SummaryPromptType::MeetingNotes) {
        SummaryPromptType::MeetingNotes => crate::l3::PromptType::MeetingNotes,
        SummaryPromptType::ComparisonTable => crate::l3::PromptType::ComparisonTable,
        SummaryPromptType::TodoList => crate::l3::PromptType::TodoList,
    }
}

/// 外部复制事件 → 历史入库 + 贪吃蛇入队（抽屉数据源, UI Spec §4.2/§4.3）
///
/// 返回 `true` 表示已入库；空白文本不入库。贪吃蛇仅在收集槽开启时入队。
pub fn record_copy_entry(
    history: &Mutex<HistoryDb>,
    stack: &Mutex<StackQueue>,
    source_text: &str,
    source_app_name: &str,
) -> bool {
    if source_text.trim().is_empty() {
        return false;
    }
    let item = crate::storage::make_item(source_text, source_app_name, None);
    let _ = history.lock().unwrap().insert_item(&item);
    let mut st = stack.lock().unwrap();
    if st.is_active() {
        st.push(crate::stack::StackEntry {
            id: item.id.clone(),
            plain_text: source_text.to_string(),
            source_app_name: source_app_name.to_string(),
        });
    }
    true
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
    app: tauri::AppHandle,
    state: tauri::State<'_, IpcState>,
    l3: tauri::State<'_, std::sync::Arc<crate::l3_ipc::L3IpcState>>,
    engine: tauri::State<'_, std::sync::Arc<crate::engine::EngineState>>,
    args: ConsumeStackArgs,
) -> IpcResult<ConsumeStackReturn> {
    let mut stack = state.stack.lock().unwrap();
    if args.mode == ConsumeMode::L3AiSummary {
        // P3-2: 贪吃蛇智能汇总 — 队列文本拼接送 L3 流式，队列保持不清空（汇总完成由用户决定上屏）
        if stack.is_empty() {
            return Err(IpcError::new("STACK_EMPTY", "stack queue is empty"));
        }
        let source_text = stack_source_text(&stack);
        let consumed_count = stack.len();
        drop(stack);
        let prompt_type = summary_to_prompt_type(args.summaryPromptType);
        let stream_id = crate::l3_ipc::spawn_l3_stream(&app, l3.inner(), prompt_type, &source_text)?;
        return Ok(ConsumeStackReturn {
            consumedCount: consumed_count,
            streamId: Some(stream_id),
            mergedText: None,
        });
    }
    let r = consume_stack(&mut stack, &args)?;
    // IPC Spec: JOIN_NEWLINE / DEDUP_NUMBERED 同步合并写入剪贴板 (Tier 0 等待 ⌘V)
    if let Some(text) = r.mergedText.as_deref() {
        crate::l3_ipc::write_plain_to_pasteboard(text, engine.clipboard_owner.clone());
    }
    Ok(r)
}

// —— 贪吃蛇面板命令 (UI Spec §4.2 / IPC Spec §3.2) ——

#[tauri::command]
pub fn cmd_list_stack(state: tauri::State<'_, IpcState>) -> ListStackReturn {
    let stack = state.stack.lock().unwrap();
    list_stack(&stack)
}

#[tauri::command]
pub fn cmd_remove_stack_item(
    state: tauri::State<'_, IpcState>,
    args: StackItemArgs,
) -> IpcResult<ListStackReturn> {
    let mut stack = state.stack.lock().unwrap();
    remove_stack_item(&mut stack, args.index)
}

#[tauri::command]
pub fn cmd_reorder_stack_items(
    state: tauri::State<'_, IpcState>,
    args: StackReorderArgs,
) -> IpcResult<ListStackReturn> {
    let mut stack = state.stack.lock().unwrap();
    reorder_stack_items(&mut stack, args.from, args.to)
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
    fn stack_source_text_joins_entries_with_blank_line() {
        let mut q = StackQueue::new();
        q.set_active(true);
        q.push(StackEntry { id: "1".into(), plain_text: "片段一".into(), source_app_name: "Notion".into() });
        q.push(StackEntry { id: "2".into(), plain_text: "片段二".into(), source_app_name: "Notes".into() });
        assert_eq!(stack_source_text(&q), "片段一\n\n片段二");
    }

    #[test]
    fn summary_to_prompt_type_maps_and_defaults() {
        use crate::l3::PromptType;
        assert!(matches!(summary_to_prompt_type(None), PromptType::MeetingNotes));
        assert!(matches!(summary_to_prompt_type(Some(SummaryPromptType::ComparisonTable)), PromptType::ComparisonTable));
        assert!(matches!(summary_to_prompt_type(Some(SummaryPromptType::TodoList)), PromptType::TodoList));
    }

    #[test]
    fn record_copy_entry_fills_history_and_stack_when_active() {
        let history = Mutex::new(HistoryDb::in_memory().unwrap());
        let stack = Mutex::new(StackQueue::new());
        assert!(!record_copy_entry(&history, &stack, "   ", "X")); // 空白不入库
        stack.lock().unwrap().set_active(true);
        assert!(record_copy_entry(&history, &stack, "片段内容", "Notion"));
        // 历史已入库
        assert_eq!(history.lock().unwrap().recent(10).unwrap().len(), 1);
        // 贪吃蛇已入队 (收集槽开启)
        let st = list_stack(&stack.lock().unwrap());
        assert_eq!(st.items.len(), 1);
        assert_eq!(st.items[0].plain_text, "片段内容");
    }

    #[test]
    fn record_copy_entry_skips_stack_when_inactive() {
        let history = Mutex::new(HistoryDb::in_memory().unwrap());
        let stack = Mutex::new(StackQueue::new()); // 未开启
        assert!(record_copy_entry(&history, &stack, "内容", "Notes"));
        assert_eq!(history.lock().unwrap().recent(10).unwrap().len(), 1);
        assert_eq!(stack.lock().unwrap().len(), 0); // 未开启不入队
    }

    #[test]
    fn stack_panel_remove_and_reorder_ops() {
        let mut stack = StackQueue::new();
        stack.set_active(true);
        for t in ["a", "b", "c"] {
            stack.push(StackEntry { id: t.into(), plain_text: t.into(), source_app_name: "X".into() });
        }
        let r = reorder_stack_items(&mut stack, 2, 0).unwrap();
        assert_eq!(r.items[0].plain_text, "c");
        let r = remove_stack_item(&mut stack, 1).unwrap();
        assert_eq!(r.items.len(), 2);
        assert_eq!(r.items[1].plain_text, "b");
        assert_eq!(remove_stack_item(&mut stack, 9).unwrap_err().code, "STACK_ITEM_NOT_FOUND");
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
