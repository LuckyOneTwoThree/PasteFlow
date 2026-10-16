//! SQLite3 + FTS5(trigram) 存储实现 (TDD v1.0 §5 Storage Schema)
//!
//! - WAL + synchronous=NORMAL
//! - `clipboard_fts` 为外部内容表 (content='clipboard_items')，AFTER INSERT/DELETE/UPDATE 三触发器同步
//! - 检索：查询 ≥3 字符走 FTS5 trigram 索引；1–2 字符回退 `LIKE '%q%'` + LIMIT 50

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 历史条目 (TDD §5 `clipboard_items`)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipboardItem {
    /// UUID v7
    pub id: String,
    /// Unix ms
    pub created_at: i64,
    /// BLAKE3 hex
    pub content_hash: String,
    /// 'text' | 'html' | 'rtf' | 'image' | 'files'
    pub primary_type: String,
    pub raw_plain_text: Option<String>,
    pub raw_html: Option<String>,
    pub image_rel_path: Option<String>,
    pub source_app_name: String,
    pub source_bundle_id: Option<String>,
    /// 'exact' | 'stale'
    pub source_confidence: String,
    /// JSON array of ContentTags
    pub tags_json: String,
    /// 'DYNAMIC_BY_TARGET' | 'PASSTHROUGH' | 'CLEANED' | 'SANITIZED_OPT_IN'
    pub plain_text_policy: String,
    /// 'FULL_MULTI_CHANNEL' | 'LIGHTWEIGHT_TEXT_ONLY'
    pub payload_tier: String,
    pub is_pinned: bool,
    pub in_stack: bool,
    pub stack_order: Option<i64>,
}

/// 供给审计记录 (TDD §5 `served_history_log`)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServedLogEntry {
    pub item_id: String,
    /// 'html' | 'rtf' | 'plainText'
    pub channel: String,
    pub frontmost_bundle_id: String,
    /// 'rich_office' | 'im_cleaned_text' | 'raw_passthrough' | 'html_suppressed' | 'rtf_suppressed'
    pub applied_variant: String,
    pub duration_us: i64,
    pub degraded_timeout: bool,
}

/// FormatGate 拦截原因 (TDD §5 `gate_skip_metrics`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateSkipReason {
    G1aComplexObject,
    G1bStandaloneMedia,
    G2MultiItem,
    G3FileUrl,
    G4LegacyOnly,
    G5Oversize,
}

impl GateSkipReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::G1aComplexObject => "G1A_COMPLEX_OBJECT",
            Self::G1bStandaloneMedia => "G1B_STANDALONE_MEDIA",
            Self::G2MultiItem => "G2_MULTI_ITEM",
            Self::G3FileUrl => "G3_FILE_URL",
            Self::G4LegacyOnly => "G4_LEGACY_ONLY",
            Self::G5Oversize => "G5_OVERSIZE",
        }
    }
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS clipboard_items (
    id TEXT PRIMARY KEY,
    created_at INTEGER NOT NULL,
    content_hash TEXT NOT NULL,
    primary_type TEXT NOT NULL,
    raw_plain_text TEXT,
    raw_html TEXT,
    image_rel_path TEXT,
    source_app_name TEXT NOT NULL,
    source_bundle_id TEXT,
    source_confidence TEXT NOT NULL,
    tags_json TEXT NOT NULL,
    plain_text_policy TEXT NOT NULL,
    payload_tier TEXT NOT NULL DEFAULT 'FULL_MULTI_CHANNEL',
    is_pinned INTEGER NOT NULL DEFAULT 0,
    in_stack INTEGER NOT NULL DEFAULT 0,
    stack_order INTEGER
);
CREATE INDEX IF NOT EXISTS idx_clipboard_created_at ON clipboard_items(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_clipboard_hash ON clipboard_items(content_hash);

CREATE VIRTUAL TABLE IF NOT EXISTS clipboard_fts USING fts5(
    raw_plain_text,
    source_app_name,
    content='clipboard_items',
    content_rowid='rowid',
    tokenize='trigram'
);

CREATE TRIGGER IF NOT EXISTS clipboard_fts_ai AFTER INSERT ON clipboard_items BEGIN
    INSERT INTO clipboard_fts(rowid, raw_plain_text, source_app_name)
    VALUES (new.rowid, COALESCE(new.raw_plain_text, ''), new.source_app_name);
END;

CREATE TRIGGER IF NOT EXISTS clipboard_fts_ad AFTER DELETE ON clipboard_items BEGIN
    INSERT INTO clipboard_fts(clipboard_fts, rowid, raw_plain_text, source_app_name)
    VALUES ('delete', old.rowid, COALESCE(old.raw_plain_text, ''), old.source_app_name);
END;

CREATE TRIGGER IF NOT EXISTS clipboard_fts_au AFTER UPDATE OF raw_plain_text, source_app_name ON clipboard_items BEGIN
    INSERT INTO clipboard_fts(clipboard_fts, rowid, raw_plain_text, source_app_name)
    VALUES ('delete', old.rowid, COALESCE(old.raw_plain_text, ''), old.source_app_name);
    INSERT INTO clipboard_fts(rowid, raw_plain_text, source_app_name)
    VALUES (new.rowid, COALESCE(new.raw_plain_text, ''), new.source_app_name);
END;

CREATE TABLE IF NOT EXISTS served_history_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    item_id TEXT NOT NULL,
    served_at INTEGER NOT NULL,
    channel TEXT NOT NULL,
    frontmost_bundle_id TEXT NOT NULL,
    applied_variant TEXT NOT NULL,
    duration_us INTEGER NOT NULL,
    degraded_timeout INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS gate_skip_metrics (
    day_bucket TEXT NOT NULL,
    skip_reason TEXT NOT NULL,
    hit_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (day_bucket, skip_reason)
);
"#;

/// 存储句柄 (单连接, 外部用 Mutex<HistoryDb> 或每次调用打开)
pub struct HistoryDb {
    conn: Connection,
}

impl HistoryDb {
    /// 打开 (或创建) 数据库并初始化 schema、WAL
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        Self::from_connection(Connection::open(path)?)
    }

    /// 从已有连接初始化 (测试用 :memory:)
    pub fn from_connection(conn: Connection) -> rusqlite::Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    /// 内存库 (测试)
    pub fn in_memory() -> rusqlite::Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    /// 底层连接 (供 `metrics` 等聚合查询复用)
    pub(crate) fn connection(&self) -> &Connection {
        &self.conn
    }

    /// 插入历史条目 (DLP 命中项由调用方拦截, 不得入表)
    pub fn insert_item(&self, item: &ClipboardItem) -> rusqlite::Result<()> {
        self.conn.execute(
            r#"INSERT INTO clipboard_items
               (id, created_at, content_hash, primary_type, raw_plain_text, raw_html,
                image_rel_path, source_app_name, source_bundle_id, source_confidence,
                tags_json, plain_text_policy, payload_tier, is_pinned, in_stack, stack_order)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)"#,
            rusqlite::params![
                item.id,
                item.created_at,
                item.content_hash,
                item.primary_type,
                item.raw_plain_text,
                item.raw_html,
                item.image_rel_path,
                item.source_app_name,
                item.source_bundle_id,
                item.source_confidence,
                item.tags_json,
                item.plain_text_policy,
                item.payload_tier,
                item.is_pinned as i64,
                item.in_stack as i64,
                item.stack_order,
            ],
        )?;
        Ok(())
    }

    /// 按 hash 查找 (防死循环快照/双击判定辅助)
    pub fn find_by_hash(&self, hash: &str) -> rusqlite::Result<Option<ClipboardItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, created_at, content_hash, primary_type, raw_plain_text, raw_html,
                    image_rel_path, source_app_name, source_bundle_id, source_confidence,
                    tags_json, plain_text_policy, payload_tier, is_pinned, in_stack, stack_order
             FROM clipboard_items WHERE content_hash = ?1 LIMIT 1",
        )?;
        let mut rows = stmt.query(rusqlite::params![hash])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_to_item(row)?)),
            None => Ok(None),
        }
    }

    /// 删除条目 (级联清 FTS)
    pub fn delete_item(&self, id: &str) -> rusqlite::Result<()> {
        self.conn
            .execute("DELETE FROM clipboard_items WHERE id = ?1", rusqlite::params![id])?;
        Ok(())
    }

    /// 置顶/取消置顶
    pub fn set_pinned(&self, id: &str, pinned: bool) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE clipboard_items SET is_pinned = ?2 WHERE id = ?1",
            rusqlite::params![id, pinned as i64],
        )?;
        Ok(())
    }

    /// 最近 N 条历史 (抽屉默认视图)
    pub fn recent(&self, limit: i64) -> rusqlite::Result<Vec<ClipboardItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, created_at, content_hash, primary_type, raw_plain_text, raw_html,
                    image_rel_path, source_app_name, source_bundle_id, source_confidence,
                    tags_json, plain_text_policy, payload_tier, is_pinned, in_stack, stack_order
             FROM clipboard_items ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(rusqlite::params![limit], row_to_item)?;
        rows.collect()
    }

    /// 检索: ≥3 字符走 FTS5 trigram；1–2 字符回退 LIKE + LIMIT 50 (TDD §5)
    pub fn search(&self, query: &str, limit: i64) -> rusqlite::Result<Vec<ClipboardItem>> {
        let q = query.trim();
        if q.is_empty() {
            return self.recent(limit);
        }
        let cols = "id, created_at, content_hash, primary_type, raw_plain_text, raw_html,
                    image_rel_path, source_app_name, source_bundle_id, source_confidence,
                    tags_json, plain_text_policy, payload_tier, is_pinned, in_stack, stack_order";
        if q.chars().count() >= 3 {
            let sql = format!(
                "SELECT {cols} FROM clipboard_items WHERE rowid IN (
                     SELECT rowid FROM clipboard_fts WHERE clipboard_fts MATCH ?1
                 ) ORDER BY created_at DESC LIMIT ?2"
            );
            let mut stmt = self.conn.prepare(&sql)?;
            let rows = stmt.query_map(rusqlite::params![q, limit], row_to_item)?;
            rows.collect()
        } else {
            let pattern = format!("%{}%", q.replace('%', "\\%").replace('_', "\\_"));
            let sql = format!(
                "SELECT {cols} FROM clipboard_items
                 WHERE raw_plain_text LIKE ?1 ESCAPE '\\' OR source_app_name LIKE ?1 ESCAPE '\\'
                 ORDER BY created_at DESC LIMIT ?2"
            );
            let mut stmt = self.conn.prepare(&sql)?;
            let rows = stmt.query_map(rusqlite::params![pattern, limit], row_to_item)?;
            rows.collect()
        }
    }

    /// 供给审计记录 (诊断面板 §11.4)
    pub fn log_served(&self, entry: &ServedLogEntry) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO served_history_log
             (item_id, served_at, channel, frontmost_bundle_id, applied_variant, duration_us, degraded_timeout)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                entry.item_id,
                now_ms(),
                entry.channel,
                entry.frontmost_bundle_id,
                entry.applied_variant,
                entry.duration_us,
                entry.degraded_timeout as i64,
            ],
        )?;
        Ok(())
    }

    /// 护栏拦截计数 (按日聚合, 不含内容)
    pub fn bump_gate_skip(&self, day_bucket: &str, reason: GateSkipReason) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO gate_skip_metrics (day_bucket, skip_reason, hit_count) VALUES (?1, ?2, 1)
             ON CONFLICT(day_bucket, skip_reason) DO UPDATE SET hit_count = hit_count + 1",
            rusqlite::params![day_bucket, reason.as_str()],
        )?;
        Ok(())
    }

    /// 今日拦截计数 (诊断面板)
    pub fn gate_skip_count(&self, day_bucket: &str, reason: GateSkipReason) -> rusqlite::Result<i64> {
        self.conn.query_row(
            "SELECT hit_count FROM gate_skip_metrics WHERE day_bucket = ?1 AND skip_reason = ?2",
            rusqlite::params![day_bucket, reason.as_str()],
            |row| row.get(0),
        )
        .or(Ok(0))
    }
}

fn row_to_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<ClipboardItem> {
    Ok(ClipboardItem {
        id: row.get(0)?,
        created_at: row.get(1)?,
        content_hash: row.get(2)?,
        primary_type: row.get(3)?,
        raw_plain_text: row.get(4)?,
        raw_html: row.get(5)?,
        image_rel_path: row.get(6)?,
        source_app_name: row.get(7)?,
        source_bundle_id: row.get(8)?,
        source_confidence: row.get(9)?,
        tags_json: row.get(10)?,
        plain_text_policy: row.get(11)?,
        payload_tier: row.get(12)?,
        is_pinned: row.get::<_, i64>(13)? != 0,
        in_stack: row.get::<_, i64>(14)? != 0,
        stack_order: row.get(15)?,
    })
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 构造一个历史条目 (供引擎/测试便捷调用)
pub fn make_item(
    plain: &str,
    source_app_name: &str,
    source_bundle_id: Option<&str>,
) -> ClipboardItem {
    ClipboardItem {
        id: uuid::Uuid::now_v7().to_string(),
        created_at: now_ms(),
        content_hash: blake3::hash(plain.as_bytes()).to_hex().to_string(),
        primary_type: "text".into(),
        raw_plain_text: Some(plain.to_string()),
        raw_html: None,
        image_rel_path: None,
        source_app_name: source_app_name.into(),
        source_bundle_id: source_bundle_id.map(|s| s.to_string()),
        source_confidence: "exact".into(),
        tags_json: "[]".into(),
        plain_text_policy: "PASSTHROUGH".into(),
        payload_tier: "FULL_MULTI_CHANNEL".into(),
        is_pinned: false,
        in_stack: false,
        stack_order: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(plain: &str) -> ClipboardItem {
        make_item(plain, "Notion", Some("notion.id"))
    }

    #[test]
    fn insert_and_recent() {
        let db = HistoryDb::in_memory().unwrap();
        for i in 0..5 {
            db.insert_item(&sample(&format!("line {i}"))).unwrap();
        }
        let items = db.recent(10).unwrap();
        assert_eq!(items.len(), 5);
    }

    #[test]
    fn chinese_substring_search_via_trigram() {
        // TDD §5: trigram 原生支持中文任意 ≥3 字符子串
        let db = HistoryDb::in_memory().unwrap();
        db.insert_item(&sample("架构周报总结与下周计划")).unwrap();
        db.insert_item(&sample("unrelated content")).unwrap();
        let hits = db.search("周报总结", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].raw_plain_text.as_deref().unwrap().contains("周报"));
    }

    #[test]
    fn short_query_falls_back_to_like() {
        // 1–2 字符短查询回退 LIKE
        let db = HistoryDb::in_memory().unwrap();
        db.insert_item(&sample("周报")).unwrap();
        db.insert_item(&sample("其他")).unwrap();
        let hits = db.search("周报", 10).unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn delete_cascades_fts() {
        let db = HistoryDb::in_memory().unwrap();
        let item = sample("architecture notes");
        db.insert_item(&item).unwrap();
        assert_eq!(db.search("architecture", 10).unwrap().len(), 1);
        db.delete_item(&item.id).unwrap();
        assert!(db.search("architecture", 10).unwrap().is_empty());
    }

    #[test]
    fn find_by_hash() {
        let db = HistoryDb::in_memory().unwrap();
        let item = sample("hello");
        db.insert_item(&item).unwrap();
        assert!(db.find_by_hash(&item.content_hash).unwrap().is_some());
        assert!(db.find_by_hash("deadbeef").unwrap().is_none());
    }

    #[test]
    fn pinned_flag_roundtrip() {
        let db = HistoryDb::in_memory().unwrap();
        let item = sample("pinned me");
        db.insert_item(&item).unwrap();
        db.set_pinned(&item.id, true).unwrap();
        assert!(db.find_by_hash(&item.content_hash).unwrap().unwrap().is_pinned);
    }

    #[test]
    fn gate_skip_metrics_accumulate() {
        let db = HistoryDb::in_memory().unwrap();
        let day = "2026-10-16";
        db.bump_gate_skip(day, GateSkipReason::G1aComplexObject).unwrap();
        db.bump_gate_skip(day, GateSkipReason::G1aComplexObject).unwrap();
        assert_eq!(
            db.gate_skip_count(day, GateSkipReason::G1aComplexObject).unwrap(),
            2
        );
    }

    #[test]
    fn served_log_records() {
        let db = HistoryDb::in_memory().unwrap();
        db.insert_item(&sample("served")).unwrap();
        db.log_served(&ServedLogEntry {
            item_id: "x".into(),
            channel: "plainText".into(),
            frontmost_bundle_id: "com.apple.Terminal".into(),
            applied_variant: "raw_passthrough".into(),
            duration_us: 42,
            degraded_timeout: false,
        })
        .unwrap();
    }
}
