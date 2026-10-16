//! P2-6 贪吃蛇 Stack Mode (UI Spec v1.0 §4.2 / IPC Spec v1.0 `cmd_set_stack_mode`)
//!
//! 连续收集槽：开启 `⌥⇧S` 后每次复制片段进入 `StackQueue[1..N]`，支持拖拽排序、
//! 单条移除与三种一键消费：
//!   - `JOIN_NEWLINE` 换行合并粘贴 (主按钮, `⌘↩`)
//!   - `DEDUP_NUMBERED` 去重编号列表
//!   - `L3_AI_SUMMARY` AI 智能汇总 (P3 流式, 预留)
//!
//! 消费后写入剪贴板并清空队列 (IPC Spec `cmd_consume_stack_queue`)。

use serde::{Deserialize, Serialize};

/// 一键消费模式 (IPC Spec `cmd_consume_stack_queue.mode`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConsumeMode {
    JoinNewline,
    DedupNumbered,
    L3AiSummary,
}

/// 汇总提示词类型 (IPC Spec `summaryPromptType`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SummaryPromptType {
    MeetingNotes,
    ComparisonTable,
    TodoList,
}

/// 队列片段 (StackQueue[i])
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackEntry {
    pub id: String,
    pub plain_text: String,
    pub source_app_name: String,
}

/// 消费结果
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsumeResult {
    pub consumed_count: usize,
    /// 合并后的文本 (JOIN_NEWLINE / DEDUP_NUMBERED); L3 模式为 None
    pub merged_text: Option<String>,
    /// L3 流式会话 id (P3 预留)
    pub stream_id: Option<String>,
}

/// 贪吃蛇收集槽队列
#[derive(Debug, Clone, Default)]
pub struct StackQueue {
    active: bool,
    entries: Vec<StackEntry>,
}

impl StackQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// 开关收集槽 (`⌥⇧S`)；关闭时清空队列 (UI Spec §4.2 面板随开启展开/收起)
    pub fn set_active(&mut self, active: bool) -> bool {
        self.active = active;
        if !active {
            self.entries.clear();
        }
        active
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 入队 (仅收集槽开启时)
    pub fn push(&mut self, entry: StackEntry) -> bool {
        if !self.active {
            return false;
        }
        self.entries.push(entry);
        true
    }

    /// 单条移除 (UI Spec §4.2)
    pub fn remove_at(&mut self, index: usize) -> Option<StackEntry> {
        if index < self.entries.len() {
            Some(self.entries.remove(index))
        } else {
            None
        }
    }

    /// 拖拽排序 (from -> to, 0-based)
    pub fn reorder(&mut self, from: usize, to: usize) -> bool {
        if from >= self.entries.len() || to >= self.entries.len() {
            return false;
        }
        let e = self.entries.remove(from);
        self.entries.insert(to, e);
        true
    }

    pub fn entries(&self) -> &[StackEntry] {
        &self.entries
    }

    /// 一键消费 (IPC Spec `cmd_consume_stack_queue`)
    ///
    /// - `JoinNewline` / `DedupNumbered`：同步合并文本，返回 `merged_text` 并清空队列
    /// - `L3AiSummary`：P3 流式预留（返回 `stream_id`, 不清空队列）
    pub fn consume(&mut self, mode: ConsumeMode, _prompt: Option<SummaryPromptType>) -> ConsumeResult {
        match mode {
            ConsumeMode::JoinNewline | ConsumeMode::DedupNumbered => {
                let consumed_count = self.entries.len();
                let merged = match mode {
                    ConsumeMode::JoinNewline => self.entries
                        .iter()
                        .map(|e| e.plain_text.as_str())
                        .collect::<Vec<_>>()
                        .join("\n"),
                    _ => dedup_numbered(&self.entries),
                };
                self.entries.clear();
                ConsumeResult {
                    consumed_count,
                    merged_text: Some(merged),
                    stream_id: None,
                }
            }
            ConsumeMode::L3AiSummary => {
                // P3 预留: 流式聚合不在此同步执行, 队列保持
                ConsumeResult {
                    consumed_count: 0,
                    merged_text: None,
                    stream_id: Some("p3-pending".into()),
                }
            }
        }
    }
}

/// 去重编号列表: 首次出现的内容去重后按序编号
fn dedup_numbered(entries: &[StackEntry]) -> String {
    let mut seen = std::collections::HashSet::new();
    let mut lines = Vec::new();
    for e in entries {
        let text = e.plain_text.trim();
        if text.is_empty() || !seen.insert(text) {
            continue;
        }
        lines.push(format!("{}. {}", lines.len() + 1, text));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, text: &str) -> StackEntry {
        StackEntry {
            id: id.into(),
            plain_text: text.into(),
            source_app_name: "Notion".into(),
        }
    }

    #[test]
    fn inactive_queue_rejects_push() {
        let mut q = StackQueue::new();
        assert!(!q.push(entry("1", "hello")));
        assert!(q.is_empty());
    }

    #[test]
    fn activate_accepts_push() {
        let mut q = StackQueue::new();
        assert!(q.set_active(true));
        assert!(q.push(entry("1", "hello")));
        assert!(q.push(entry("2", "world")));
        assert_eq!(q.len(), 2);
    }

    #[test]
    fn deactivate_clears_queue() {
        let mut q = StackQueue::new();
        q.set_active(true);
        q.push(entry("1", "hello"));
        q.set_active(false);
        assert!(q.is_empty());
    }

    #[test]
    fn remove_at_single_item() {
        let mut q = StackQueue::new();
        q.set_active(true);
        q.push(entry("1", "a"));
        q.push(entry("2", "b"));
        assert_eq!(q.remove_at(0).unwrap().plain_text, "a");
        assert_eq!(q.len(), 1);
        assert!(q.remove_at(5).is_none());
    }

    #[test]
    fn reorder_moves_entry() {
        let mut q = StackQueue::new();
        q.set_active(true);
        for t in ["a", "b", "c"] {
            q.push(entry(t, t));
        }
        assert!(q.reorder(2, 0));
        assert_eq!(q.entries()[0].plain_text, "c");
        assert!(!q.reorder(0, 9));
    }

    #[test]
    fn consume_join_newline_merges_and_clears() {
        let mut q = StackQueue::new();
        q.set_active(true);
        q.push(entry("1", "hello"));
        q.push(entry("2", "world"));
        let r = q.consume(ConsumeMode::JoinNewline, None);
        assert_eq!(r.consumed_count, 2);
        assert_eq!(r.merged_text.as_deref(), Some("hello\nworld"));
        assert!(q.is_empty());
    }

    #[test]
    fn consume_dedup_numbered_dedups() {
        let mut q = StackQueue::new();
        q.set_active(true);
        q.push(entry("1", "alpha"));
        q.push(entry("2", "beta"));
        q.push(entry("3", "alpha")); // 重复
        q.push(entry("4", "")); // 空白忽略
        let r = q.consume(ConsumeMode::DedupNumbered, None);
        assert_eq!(r.consumed_count, 4);
        assert_eq!(r.merged_text.as_deref(), Some("1. alpha\n2. beta"));
        assert!(q.is_empty());
    }

    #[test]
    fn consume_l3_is_reserved_and_keeps_queue() {
        let mut q = StackQueue::new();
        q.set_active(true);
        q.push(entry("1", "keep"));
        let r = q.consume(ConsumeMode::L3AiSummary, Some(SummaryPromptType::MeetingNotes));
        assert_eq!(r.consumed_count, 0);
        assert!(r.stream_id.is_some());
        assert_eq!(q.len(), 1); // P3 流式消费, 队列保持
    }

    #[test]
    fn chinese_dedup_works() {
        let mut q = StackQueue::new();
        q.set_active(true);
        q.push(entry("1", "架构周报"));
        q.push(entry("2", "架构周报"));
        let r = q.consume(ConsumeMode::DedupNumbered, None);
        assert_eq!(r.merged_text.as_deref(), Some("1. 架构周报"));
    }
}
