//! P2-5 历史抽屉逻辑 (UI Spec v1.0 §4)
//!
//! 磁吸式侧边抽屉（360px，`⌥⇧V` 唤醒）：
//!   - 顶部搜索栏：SQLite FTS5 trigram 子串毫秒级过滤（复用 `storage::HistoryDb::search`）
//!   - 类型胶囊过滤：`全部` | `✨ Markdown` | `🔗 链接` | `💻 代码` | `🖼️ 图片`
//!   - 条目卡片：Before/After 就地预览切换（不污染剪贴板与库状态）
//!
//! 本模块只做纯逻辑（过滤/预览状态）；面板布局与渲染属 Webview 层。

use crate::storage::ClipboardItem;
use serde::{Deserialize, Serialize};

/// 类型胶囊过滤标签 (UI Spec §4.2)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TypeFilter {
    All,
    Markdown,
    Link,
    Code,
    Image,
}

impl TypeFilter {
    /// 判断条目是否匹配过滤标签
    pub fn matches(&self, item: &ClipboardItem) -> bool {
        match self {
            Self::All => true,
            Self::Markdown => item.tags_json.contains("markdown") || item.primary_type == "text",
            Self::Link => item.tags_json.contains("link"),
            Self::Code => item.tags_json.contains("code"),
            Self::Image => item.primary_type == "image",
        }
    }
}

/// Before/After 就地预览态 (UI Spec §4.3)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreviewMode {
    /// 原始文本 (Before)
    Before,
    /// 富文本渲染效果 (After)
    After,
}

impl Default for PreviewMode {
    fn default() -> Self {
        Self::Before
    }
}

/// 条目卡片预览状态（每条目独立）
#[derive(Debug, Clone, Default)]
pub struct PreviewState {
    mode: PreviewMode,
}

impl PreviewState {
    pub fn new() -> Self {
        Self::default()
    }

    /// 切换 Before/After（悬停显示的「预览转换效果」按钮触发）
    pub fn toggle(&mut self) -> PreviewMode {
        self.mode = match self.mode {
            PreviewMode::Before => PreviewMode::After,
            PreviewMode::After => PreviewMode::Before,
        };
        self.mode
    }

    pub fn mode(&self) -> PreviewMode {
        self.mode
    }
}

/// 抽屉视图模型：搜索词 + 过滤标签 -> 有序卡片列表
#[derive(Debug, Clone)]
pub struct DrawerViewModel {
    pub query: String,
    pub filter: TypeFilter,
}

impl DrawerViewModel {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            filter: TypeFilter::All,
        }
    }

    /// 清空搜索词（唤醒时聚焦搜索栏, UI Spec §4.2）
    pub fn clear_query(&mut self) {
        self.query.clear();
    }

    /// 从检索结果应用类型过滤（检索已由 `HistoryDb::search` 负责）
    pub fn apply(&self, results: Vec<ClipboardItem>) -> Vec<ClipboardItem> {
        results.into_iter().filter(|it| self.filter.matches(it)).collect()
    }
}

impl Default for DrawerViewModel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::make_item;

    fn item_with_tags(tags: &str, primary: &str) -> ClipboardItem {
        let mut it = make_item("content", "Notion", Some("notion.id"));
        it.tags_json = tags.into();
        it.primary_type = primary.into();
        it
    }

    #[test]
    fn filter_all_passes_everything() {
        let f = TypeFilter::All;
        assert!(f.matches(&item_with_tags("[]", "text")));
        assert!(f.matches(&item_with_tags("[]", "image")));
    }

    #[test]
    fn filter_markdown_matches_text() {
        let f = TypeFilter::Markdown;
        assert!(f.matches(&item_with_tags("[]", "text")));
        assert!(f.matches(&item_with_tags(r#"["markdown"]"#, "text")));
        assert!(!f.matches(&item_with_tags("[]", "image")));
    }

    #[test]
    fn filter_link_and_code_by_tags() {
        assert!(TypeFilter::Link.matches(&item_with_tags(r#"["link"]"#, "text")));
        assert!(TypeFilter::Code.matches(&item_with_tags(r#"["code"]"#, "text")));
        assert!(!TypeFilter::Link.matches(&item_with_tags("[]", "text")));
    }

    #[test]
    fn filter_image_by_primary_type() {
        assert!(TypeFilter::Image.matches(&item_with_tags("[]", "image")));
        assert!(!TypeFilter::Image.matches(&item_with_tags("[]", "text")));
    }

    #[test]
    fn preview_toggles_before_after() {
        let mut p = PreviewState::new();
        assert_eq!(p.mode(), PreviewMode::Before);
        assert_eq!(p.toggle(), PreviewMode::After);
        assert_eq!(p.toggle(), PreviewMode::Before);
    }

    #[test]
    fn drawer_view_model_applies_filter() {
        let mut vm = DrawerViewModel::new();
        vm.filter = TypeFilter::Image;
        let items = vec![item_with_tags("[]", "text"), item_with_tags("[]", "image")];
        assert_eq!(vm.apply(items).len(), 1);
    }

    #[test]
    fn drawer_view_model_clear_query() {
        let mut vm = DrawerViewModel::new();
        vm.query = "周报".into();
        vm.clear_query();
        assert!(vm.query.is_empty());
    }
}
