//! 本地诊断面板统计指标聚合与脱敏周报导出 (TDD v1.0 §5.4 `served_history_log`/`gate_skip_metrics`; Master Spec §11.4)
//!
//! - **只统计不落正文**：本模块只做计数/分位数聚合，任何输出绝不含剪贴板正文字段
//!   (rawPlainText / contentHash / snippet 等)。
//! - 写入复用 `db.rs` 的 `log_served` / `bump_gate_skip`，本模块仅补聚合查询。
//! - `day_bucket` 口径：UTC 日期 `"YYYY-MM-DD"`，与 `served_at` (Unix ms) 同口径。

use super::db::{GateSkipReason, HistoryDb, ServedLogEntry};
use serde::Serialize;
use std::collections::BTreeMap;

/// 一天的毫秒数 (UTC 日界)
const DAY_MS: i64 = 86_400_000;

/// 单日耗时分位数 (µs, nearest-rank 法)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LatencyDto {
    pub p50_us: i64,
    pub p95_us: i64,
    pub p99_us: i64,
}

/// 单日统计汇总 (纯统计, 无正文字段)
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailySummaryDto {
    /// UTC 日 bucket ("YYYY-MM-DD")
    pub day_bucket: String,
    /// 当日供给次数 (`served_history_log` 计数)
    pub served_count: i64,
    /// 当日 FormatGate 拦截分布 (reason -> count)
    pub gate_skip: BTreeMap<String, i64>,
    /// 当日供给耗时分位数 (`duration_us`)
    pub latency_us: LatencyDto,
}

impl DailySummaryDto {
    /// 全零汇总 (空日 / 无效日期 / 查询失败兜底, 不 panic)
    fn zero(day_bucket: &str) -> Self {
        Self {
            day_bucket: day_bucket.to_string(),
            served_count: 0,
            gate_skip: BTreeMap::new(),
            latency_us: LatencyDto { p50_us: 0, p95_us: 0, p99_us: 0 },
        }
    }
}

/// 指标聚合句柄 (借用 `HistoryDb`, 复用其连接与写入方法)
pub struct MetricsStore<'a> {
    db: &'a HistoryDb,
}

impl<'a> MetricsStore<'a> {
    pub fn new(db: &'a HistoryDb) -> Self {
        Self { db }
    }

    /// 记录一次供给审计 (复用 `db.rs::log_served`)
    pub fn record_served(&self, entry: &ServedLogEntry) -> rusqlite::Result<()> {
        self.db.log_served(entry)
    }

    /// 记录一次 FormatGate 拦截 (复用 `db.rs::bump_gate_skip`)
    pub fn record_gate_skip(
        &self,
        day_bucket: &str,
        reason: GateSkipReason,
    ) -> rusqlite::Result<()> {
        self.db.bump_gate_skip(day_bucket, reason)
    }

    /// 单日汇总：gate_skip 分布 + served 计数 + p50/p95/p99 耗时
    pub fn daily_summary(&self, day_bucket: &str) -> rusqlite::Result<DailySummaryDto> {
        let Some(day_start) = day_start_ms(day_bucket) else {
            return Ok(DailySummaryDto::zero(day_bucket)); // 无效日期: 零值不 panic
        };
        let day_end = day_start + DAY_MS;
        let conn = self.db.connection();

        // —— FormatGate 拦截分布 (reason -> count) ——
        let mut gate_skip = BTreeMap::new();
        let mut stmt = conn.prepare(
            "SELECT skip_reason, hit_count FROM gate_skip_metrics
             WHERE day_bucket = ?1 ORDER BY skip_reason",
        )?;
        let rows = stmt.query_map(rusqlite::params![day_bucket], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (reason, count) = row?;
            gate_skip.insert(reason, count);
        }

        // —— 当日供给计数 + 耗时样本 (`served_at` 为 Unix ms, 按 UTC 日界切分) ——
        let served_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM served_history_log WHERE served_at >= ?1 AND served_at < ?2",
            rusqlite::params![day_start, day_end],
            |row| row.get(0),
        )?;
        let mut durations: Vec<i64> = Vec::new();
        let mut stmt = conn.prepare(
            "SELECT duration_us FROM served_history_log
             WHERE served_at >= ?1 AND served_at < ?2 ORDER BY duration_us",
        )?;
        let rows = stmt.query_map(rusqlite::params![day_start, day_end], |row| row.get(0))?;
        for row in rows {
            durations.push(row?);
        }

        Ok(DailySummaryDto {
            day_bucket: day_bucket.to_string(),
            served_count,
            gate_skip,
            latency_us: LatencyDto {
                p50_us: percentile(&durations, 50),
                p95_us: percentile(&durations, 95),
                p99_us: percentile(&durations, 99),
            },
        })
    }

    /// 脱敏周报 (Master Spec §11.4)：近 `days` 天逐日汇总数组 (纯统计, 无任何正文字段)
    ///
    /// 数组按时间升序排列，末位为 `end_day` 当日。
    pub fn weekly_report(&self, end_day: &str, days: u64) -> serde_json::Value {
        let mut daily = Vec::with_capacity(days as usize);
        for offset in (0..days).rev() {
            let day = shift_day(end_day, -(offset as i64)).unwrap_or_else(|| end_day.to_string());
            let summary = self
                .daily_summary(&day)
                .unwrap_or_else(|_| DailySummaryDto::zero(&day));
            daily.push(serde_json::to_value(&summary).unwrap_or(serde_json::Value::Null));
        }
        serde_json::json!({
            "reportType": "sanitized_weekly",
            "endDay": end_day,
            "days": days,
            "daily": daily,
        })
    }
}

/// 百分位 (样本已升序排序, nearest-rank 法, 与 `dashboard_window::percentile` 同口径)
fn percentile(sorted: &[i64], pct: usize) -> i64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = (sorted.len() * pct).div_ceil(100).max(1);
    sorted[rank - 1]
}

// —— UTC 日期工具 (纯计算, 不依赖额外 feature) ——

/// 当前 UTC 日 bucket ("YYYY-MM-DD")
pub fn today_bucket() -> String {
    day_bucket_of(now_ms())
}

/// Unix ms -> UTC 日 bucket ("YYYY-MM-DD")
pub fn day_bucket_of(ms: i64) -> String {
    let (y, m, d) = civil_from_days(ms.div_euclid(DAY_MS));
    format!("{y:04}-{m:02}-{d:02}")
}

/// UTC 日 bucket -> 当日 00:00:00Z 的 Unix ms (格式非法返回 None)
fn day_start_ms(day: &str) -> Option<i64> {
    let mut parts = day.split('-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: i64 = parts.next()?.parse().ok()?;
    let d: i64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(days_from_civil(y, m, d) * DAY_MS)
}

/// 日期偏移 (delta 可为负), 格式非法返回 None
fn shift_day(day: &str, delta: i64) -> Option<String> {
    let start = day_start_ms(day)?;
    let (y, m, d) = civil_from_days((start / DAY_MS) + delta);
    Some(format!("{y:04}-{m:02}-{d:02}"))
}

/// 日期 -> 1970-01-01 起的天数 (Howard Hinnant `days_from_civil`)
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400; // [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 }; // [0, 11]
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

/// 1970-01-01 起的天数 -> (年, 月, 日) (`days_from_civil` 的逆运算)
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 直接向 `served_history_log` 落一条指定 `served_at` 的记录 (跨日聚合测试用)
    fn insert_served_at(db: &HistoryDb, served_at: i64, duration_us: i64) {
        db.connection()
            .execute(
                "INSERT INTO served_history_log
                 (item_id, served_at, channel, frontmost_bundle_id, applied_variant, duration_us, degraded_timeout)
                 VALUES ('item-1', ?1, 'html', 'com.apple.Terminal', 'rich_office', ?2, 0)",
                rusqlite::params![served_at, duration_us],
            )
            .unwrap();
    }

    fn sample_entry(duration_us: i64) -> ServedLogEntry {
        ServedLogEntry {
            item_id: "item-1".into(),
            channel: "plain_text_channel".into(),
            frontmost_bundle_id: "com.apple.Terminal".into(),
            applied_variant: "raw_passthrough".into(),
            duration_us,
            degraded_timeout: false,
        }
    }

    #[test]
    fn daily_summary_counts_served_and_gate_distribution() {
        let db = HistoryDb::in_memory().unwrap();
        let store = MetricsStore::new(&db);
        store.record_served(&sample_entry(10)).unwrap();
        store.record_served(&sample_entry(20)).unwrap();
        store.record_served(&sample_entry(30)).unwrap();
        // day_bucket 从实际落库的 served_at 推导，避免跨 UTC 零点竞态
        let day = {
            let ms: i64 = db
                .connection()
                .query_row("SELECT MAX(served_at) FROM served_history_log", [], |r| {
                    r.get(0)
                })
                .unwrap();
            day_bucket_of(ms)
        };
        store.record_gate_skip(&day, GateSkipReason::G1aComplexObject).unwrap();
        store.record_gate_skip(&day, GateSkipReason::G1aComplexObject).unwrap();
        store.record_gate_skip(&day, GateSkipReason::G1bStandaloneMedia).unwrap();

        let s = store.daily_summary(&day).unwrap();
        assert_eq!(s.day_bucket, day);
        assert_eq!(s.served_count, 3);
        assert_eq!(s.gate_skip.get("G1A_COMPLEX_OBJECT"), Some(&2));
        assert_eq!(s.gate_skip.get("G1B_STANDALONE_MEDIA"), Some(&1));
        assert_eq!(s.gate_skip.len(), 2);
        // nearest-rank: 3 样本 [10,20,30] -> p50=20, p95=30, p99=30
        assert_eq!(s.latency_us.p50_us, 20);
        assert_eq!(s.latency_us.p95_us, 30);
        assert_eq!(s.latency_us.p99_us, 30);
    }

    #[test]
    fn percentile_nearest_rank_matches_reference() {
        // 与 dashboard_window::percentile 同口径 (nearest-rank)
        assert_eq!(percentile(&[], 95), 0);
        assert_eq!(percentile(&[1, 2, 3, 4, 5], 50), 3);
        assert_eq!(percentile(&[1, 2, 3, 4, 5], 95), 5);
        assert_eq!(percentile(&[1, 2, 3, 4, 5], 99), 5);
        assert_eq!(percentile(&[7], 50), 7);
    }

    #[test]
    fn daily_summary_separates_day_boundaries() {
        // UTC 日界切分: 前一日 23:59:59.999 与次日 00:00:00 不得串日
        let db = HistoryDb::in_memory().unwrap();
        let store = MetricsStore::new(&db);
        let start = day_start_ms("2026-10-16").unwrap();
        insert_served_at(&db, start - 1, 5);
        insert_served_at(&db, start, 15);
        insert_served_at(&db, start + DAY_MS - 1, 25);
        insert_served_at(&db, start + DAY_MS, 35);
        let s = store.daily_summary("2026-10-16").unwrap();
        assert_eq!(s.served_count, 2);
    }

    #[test]
    fn weekly_report_contains_no_content_fields() {
        // Master Spec §11.4: 脱敏周报 = 纯统计, 绝不含正文字段名
        let db = HistoryDb::in_memory().unwrap();
        let store = MetricsStore::new(&db);
        let start_14 = day_start_ms("2026-10-14").unwrap();
        insert_served_at(&db, start_14, 12);
        insert_served_at(&db, start_14 + 3_600_000, 48);
        let start_16 = day_start_ms("2026-10-16").unwrap();
        insert_served_at(&db, start_16, 20);
        store.record_gate_skip("2026-10-15", GateSkipReason::G5Oversize).unwrap();
        store.record_gate_skip("2026-10-15", GateSkipReason::G5Oversize).unwrap();

        let v = store.weekly_report("2026-10-16", 7);
        let json = v.to_string();
        let daily = v["daily"].as_array().unwrap();
        assert_eq!(daily.len(), 7);
        assert_eq!(daily[0]["dayBucket"], "2026-10-10"); // 最旧一天
        assert_eq!(daily[6]["dayBucket"], "2026-10-16"); // end_day 在末位
        assert_eq!(daily[4]["servedCount"], 2); // 10-14 两条
        assert_eq!(daily[5]["gateSkip"]["G5_OVERSIZE"], 2); // 10-15 两次拦截
        assert_eq!(daily[6]["latencyUs"]["p50Us"], 20);

        for forbidden in [
            "rawPlainText",
            "contentHash",
            "snippet",
            "plainText",
            "raw_plain_text",
            "content_hash",
        ] {
            assert!(!json.contains(forbidden), "脱敏周报泄漏正文字段: {forbidden}");
        }
    }

    #[test]
    fn daily_summary_empty_db_returns_zeros_without_panic() {
        let db = HistoryDb::in_memory().unwrap();
        let store = MetricsStore::new(&db);
        let s = store.daily_summary("2026-10-16").unwrap();
        assert_eq!(s.day_bucket, "2026-10-16");
        assert_eq!(s.served_count, 0);
        assert!(s.gate_skip.is_empty());
        assert_eq!(s.latency_us.p50_us, 0);
        assert_eq!(s.latency_us.p95_us, 0);
        assert_eq!(s.latency_us.p99_us, 0);
        // 无效日期同样零值不 panic
        let s = store.daily_summary("not-a-day").unwrap();
        assert_eq!(s.served_count, 0);
    }

    #[test]
    fn day_bucket_math_roundtrips() {
        assert_eq!(day_bucket_of(0), "1970-01-01");
        assert_eq!(day_start_ms("1970-01-01"), Some(0));
        assert_eq!(day_start_ms("2026-10-16"), Some(1_792_108_800_000));
        assert_eq!(shift_day("2026-10-16", -6), Some("2026-10-10".into()));
        assert_eq!(shift_day("2026-01-01", -1), Some("2025-12-31".into()));
        assert_eq!(shift_day("2024-02-28", 1), Some("2024-02-29".into()));
        assert_eq!(day_start_ms("2026-13-01"), None);
    }
}
