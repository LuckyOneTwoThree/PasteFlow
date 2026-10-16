//! P2-4 本地存储层 (TDD v1.0 §5 Storage Schema)
//!
//! SQLite3 + FTS5(trigram)：历史抽屉数据、供给审计与护栏拦截统计。
//! DLP 高置信命中项由 PrivacyAndLoopGuard 拦截，**绝不**进入本层落表。

pub mod db;

pub use db::{ClipboardItem, GateSkipReason, HistoryDb, ServedLogEntry};
