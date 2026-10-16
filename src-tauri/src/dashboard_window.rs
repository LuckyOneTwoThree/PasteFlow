//! P2-7 主面板窗口生命周期 (IPC Spec v1.0 §1 `main-dashboard`)
//!
//! 契约：**按需创建** `880×600` 独立 `WebviewWindow`；用户点关闭按钮后隐藏并启动
//! `30s` 倒计时，期间未再次打开则 `destroy()` 彻底释放 WebKit 渲染进程。
//!
//! 诊断看板命令（IPC Spec §3）：
//!   - `cmd_get_diagnostic_snapshot`：changeCount / 最近 20 条 ServedRecord /
//!     FormatGate G1A–G5 拦截分布 / 回调耗时分位数 (p50/p95/p99)
//!   - `cmd_export_sanitized_report`：导出纯统计 JSON（不含剪贴板正文）

use crate::storage::HistoryDb;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::{Duration, Instant};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

/// 主面板窗口标识 (IPC Spec §1)
pub const DASHBOARD_LABEL: &str = "main-dashboard";
/// 主面板页面
const DASHBOARD_URL: &str = "dashboard.html";
/// 关闭后销毁倒计时 (IPC Spec §1: 30s)
pub const DESTROY_DELAY_SECS: u64 = 30;

// —— 诊断快照 DTO (IPC Spec §3 `DiagnosticSnapshotDto`) ——

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServedRecordDto {
    pub frontmost_app: Option<String>,
    pub variant: Option<String>,
    pub elapsed_us: Option<u64>,
    /// 当前补挂代数 (0 = 原始挂载; Master Spec §3.2 #4: 诊断看板需展示)
    pub generation: Option<usize>,
    /// 供给类别: `sniffer_probe` 嗅探抢读固化 | `real_serve` 真实粘贴供给 (A.13 要求区分)
    pub serve_kind: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LatencyDto {
    pub p50_us: u64,
    pub p95_us: u64,
    pub p99_us: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticSnapshotDto {
    pub change_count: i64,
    pub today_served: i64,
    /// FormatGate 拦截统计 (G1A/G1B/G2/G3/G4/G5 -> count)
    pub gate: HashMap<String, u64>,
    /// 各滤镜有效触发次数 (§11.4 #2)
    pub filter: HashMap<String, u64>,
    pub served: Vec<ServedRecordDto>,
    pub latency: LatencyDto,
}

/// 诊断计数器（引擎写入，诊断看板读取）
#[derive(Debug, Default)]
pub struct DiagnosticCounters {
    change_count: AtomicI64,
    today_served: AtomicI64,
    /// FormatGate 拦截统计 (gate_id -> count)
    gate: std::sync::Mutex<HashMap<String, u64>>,
    /// 各滤镜有效触发次数 (filter_id -> count, §11.4 #2)
    filter: std::sync::Mutex<HashMap<String, u64>>,
    /// 最近 20 条回调日志
    served: std::sync::Mutex<Vec<ServedRecordDto>>,
    /// 回调耗时样本 (µs, 用于分位数)
    latencies: std::sync::Mutex<Vec<u64>>,
}

impl DiagnosticCounters {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_change_count(&self, cc: i64) {
        self.change_count.store(cc, Ordering::Relaxed);
    }

    pub fn record_served(&self, rec: ServedRecordDto, elapsed_us: u64) {
        self.today_served.fetch_add(1, Ordering::Relaxed);
        let mut served = self.served.lock().unwrap();
        served.insert(0, rec);
        served.truncate(20); // UI Spec §5: 最近 20 条
        drop(served);
        let mut lats = self.latencies.lock().unwrap();
        lats.push(elapsed_us);
        lats.truncate(1024);
    }

    pub fn record_gate_skip(&self, gate_id: &str) {
        *self.gate.lock().unwrap().entry(gate_id.to_string()).or_insert(0) += 1;
    }

    pub fn record_filter(&self, filter_id: &str) {
        *self.filter.lock().unwrap().entry(filter_id.to_string()).or_insert(0) += 1;
    }

    pub fn snapshot(&self) -> DiagnosticSnapshotDto {
        let mut lats = self.latencies.lock().unwrap().clone();
        lats.sort_unstable();
        DiagnosticSnapshotDto {
            change_count: self.change_count.load(Ordering::Relaxed),
            today_served: self.today_served.load(Ordering::Relaxed),
            gate: self.gate.lock().unwrap().clone(),
            filter: self.filter.lock().unwrap().clone(),
            served: self.served.lock().unwrap().clone(),
            latency: LatencyDto {
                p50_us: percentile(&lats, 50),
                p95_us: percentile(&lats, 95),
                p99_us: percentile(&lats, 99),
            },
        }
    }

    /// 导出脱敏 JSON（不含任何剪贴板正文）
    pub fn export_sanitized_json(&self) -> String {
        serde_json::to_string_pretty(&self.snapshot()).unwrap_or_default()
    }
}

/// 百分位（样本已升序排序，nearest-rank 法）
fn percentile(sorted: &[u64], pct: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = (sorted.len() * pct).div_ceil(100).max(1);
    sorted[rank - 1]
}

// —— 诊断标签映射 (§11.4: 看板展示 4 种供给变体 + 门控分布) ——

/// FormatGate 判定 → 门控 ID (dashboard.html `GATE_LABELS` = G1A/G1B/G2/G3/G4/G5)
pub fn gate_id(verdict: crate::format_gate::Stage1Verdict) -> &'static str {
    use crate::format_gate::Stage1Verdict as V;
    match verdict {
        V::PrivateObject => "G1A",
        V::PureImage => "G1B",
        V::MultiItem => "G2",
        V::FileCopy => "G3",
        V::LegacyJunk => "G4",
        V::Pass => "G5",
    }
}

/// 路由分类 → 供给变体标签 (Master Spec §11.4 #1: rich_office / im_cleaned_text / raw_passthrough / html_suppressed)
pub fn variant_label(route: crate::pipe_executor::RouteClass) -> &'static str {
    use crate::pipe_executor::RouteClass as R;
    match route {
        R::RichOffice => "rich_office",
        R::ImCleanedText => "im_cleaned_text",
        R::SuppressRich => "html_suppressed",
        R::StaticUnmanaged => "raw_passthrough",
    }
}

// —— 窗口生命周期 ——

/// 窗口可见状态跟踪（关闭倒计时需知「再次打开」）
#[derive(Debug, Default)]
pub struct DashboardTracker {
    /// 正在倒计时销毁
    pending_destroy: AtomicBool,
    /// 首次打开时间戳（供测试观察）
    opened_at: std::sync::Mutex<Option<Instant>>,
}

impl DashboardTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// 用户点关闭：隐藏并启动 30s 倒计时
    pub fn schedule_destroy(&self) -> bool {
        let was = self.pending_destroy.swap(true, Ordering::Relaxed);
        !was // 返回 true 表示首次安排
    }

    /// 30s 内再次打开：取消倒计时
    pub fn cancel_destroy(&self) -> bool {
        let was = self.pending_destroy.swap(false, Ordering::Relaxed);
        was
    }

    /// 倒计时到期：确认销毁
    pub fn take_pending_destroy(&self) -> bool {
        self.pending_destroy.swap(false, Ordering::Relaxed)
    }

    pub fn mark_opened(&self) {
        *self.opened_at.lock().unwrap() = Some(Instant::now());
    }
}

/// 打开主面板（按需创建；已存在则复用并取消销毁倒计时）
pub fn open(app: &tauri::AppHandle, tracker: &DashboardTracker) {
    tracker.cancel_destroy();
    tracker.mark_opened();
    if let Some(win) = app.get_webview_window(DASHBOARD_LABEL) {
        let _ = win.show();
        return;
    }
    let _ = WebviewWindowBuilder::new(app, DASHBOARD_LABEL, WebviewUrl::App(DASHBOARD_URL.into()))
        .title("灵贴 PasteFlow")
        .inner_size(880.0, 600.0)
        .resizable(false)
        .center()
        .visible(true)
        .build();
}

/// 用户点关闭：隐藏 + 30s 后 destroy（IPC Spec §1）
pub fn close_with_delay(app: tauri::AppHandle, tracker: std::sync::Arc<DashboardTracker>) {
    if !tracker.schedule_destroy() {
        return; // 已在倒计时中
    }
    if let Some(win) = app.get_webview_window(DASHBOARD_LABEL) {
        let _ = win.hide();
    }
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(DESTROY_DELAY_SECS));
        if tracker.take_pending_destroy() {
            if let Some(win) = app.get_webview_window(DASHBOARD_LABEL) {
                let _ = win.destroy(); // 彻底释放 WebKit 渲染进程
            }
        }
    });
}

/// 关闭前保存并导出脱敏报告 (UI Spec §5.2 `[一键导出脱敏诊断包]`)
///
/// 诊断数据不含剪贴板正文，导出目标由调用方指定。
pub fn export_sanitized_report(
    counters: &DiagnosticCounters,
    history: Option<&HistoryDb>,
    target_path: &str,
) -> Result<String, String> {
    let mut snap: serde_json::Value =
        serde_json::from_str(&counters.export_sanitized_json()).unwrap_or_default();
    // 附带统计信息（不含正文）
    if let Some(db) = history {
        // 历史库最近条目数（脱敏统计，不含正文）
        let total = db.recent(500).map_err(|e| e.to_string())?.len();
        snap["historyTotal"] = serde_json::json!(total);
    }
    snap["generatedAt"] = serde_json::json!(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    );
    let json = serde_json::to_string_pretty(&snap).unwrap_or_default();
    let path = if target_path.is_empty() {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{home}/Desktop/pasteflow-diagnostic-report.json")
    } else {
        target_path.to_string()
    };
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_label_matches_ipc_contract() {
        assert_eq!(DASHBOARD_LABEL, "main-dashboard");
    }

    #[test]
    fn destroy_delay_is_30s() {
        // IPC Spec §1: 关闭 30s 后 destroy
        assert_eq!(DESTROY_DELAY_SECS, 30);
    }

    #[test]
    fn tracker_schedules_and_cancels_destroy() {
        let t = DashboardTracker::new();
        assert!(t.schedule_destroy());
        assert!(!t.schedule_destroy()); // 重复安排短路
        assert!(t.cancel_destroy()); // 再次打开取消倒计时
        assert!(!t.take_pending_destroy());
        t.schedule_destroy();
        assert!(t.take_pending_destroy()); // 到期销毁
    }

    #[test]
    fn counters_snapshot_collects_gate_and_latency() {
        let c = DiagnosticCounters::new();
        c.record_change_count(42);
        c.record_gate_skip("G1B");
        c.record_gate_skip("G1B");
        c.record_gate_skip("G2");
        c.record_filter("markdown-to-im");
        c.record_served(
            ServedRecordDto {
                frontmost_app: Some("com.microsoft.Word".into()),
                variant: Some("rich_office".into()),
                elapsed_us: Some(12),
                generation: Some(0),
                serve_kind: Some("real_serve".into()),
            },
            12,
        );
        let s = c.snapshot();
        assert_eq!(s.change_count, 42);
        assert_eq!(s.today_served, 1);
        assert_eq!(s.gate["G1B"], 2);
        assert_eq!(s.gate["G2"], 1);
        assert_eq!(s.filter["markdown-to-im"], 1);
        assert_eq!(s.served.len(), 1);
        assert_eq!(s.latency.p95_us, 12);
    }

    #[test]
    fn served_log_truncates_to_20() {
        let c = DiagnosticCounters::new();
        for i in 0..25 {
            c.record_served(
                ServedRecordDto {
                    frontmost_app: Some(format!("app{i}")),
                    variant: Some("raw_passthrough".into()),
                    elapsed_us: Some(i),
                    generation: Some((i % 4) as usize),
                    serve_kind: Some("real_serve".into()),
                },
                i,
            );
        }
        assert_eq!(c.snapshot().served.len(), 20);
    }

    #[test]
    fn percentile_computes() {
        assert_eq!(percentile(&[], 95), 0);
        assert_eq!(percentile(&[1, 2, 3, 4, 5], 50), 3);
        assert_eq!(percentile(&[1, 2, 3, 4, 5], 95), 5);
    }

    #[test]
    fn export_sanitized_json_excludes_content() {
        let c = DiagnosticCounters::new();
        c.record_served(
            ServedRecordDto {
                frontmost_app: Some("com.tencent.xinWeChat".into()),
                variant: Some("im_cleaned_text".into()),
                elapsed_us: Some(8),
                generation: Some(1),
                serve_kind: Some("sniffer_probe".into()),
            },
            8,
        );
        let json = c.export_sanitized_json();
        // 脱敏: 不含任何剪贴板正文字段
        assert!(!json.contains("rawPlainText"));
        assert!(!json.contains("contentHash"));
        assert!(json.contains("com.tencent.xinWeChat"));
    }

    #[test]
    fn gate_id_maps_stage1_verdicts() {
        use crate::format_gate::Stage1Verdict as V;
        assert_eq!(gate_id(V::PrivateObject), "G1A");
        assert_eq!(gate_id(V::PureImage), "G1B");
        assert_eq!(gate_id(V::MultiItem), "G2");
        assert_eq!(gate_id(V::FileCopy), "G3");
        assert_eq!(gate_id(V::LegacyJunk), "G4");
        assert_eq!(gate_id(V::Pass), "G5");
    }

    #[test]
    fn variant_label_covers_four_contract_variants() {
        use crate::pipe_executor::RouteClass as R;
        assert_eq!(variant_label(R::RichOffice), "rich_office");
        assert_eq!(variant_label(R::ImCleanedText), "im_cleaned_text");
        assert_eq!(variant_label(R::SuppressRich), "html_suppressed");
        assert_eq!(variant_label(R::StaticUnmanaged), "raw_passthrough");
    }

    #[test]
    fn export_sanitized_report_writes_stats_only_json() {
        // §11.4 #3: 一键脱敏周报 = 纯统计 JSON, 不含剪贴板文本内容
        let c = DiagnosticCounters::new();
        c.record_gate_skip("G1B");
        c.record_served(
            ServedRecordDto {
                frontmost_app: Some("com.microsoft.Word".into()),
                variant: Some("rich_office".into()),
                elapsed_us: Some(30),
                generation: Some(2),
                serve_kind: Some("real_serve".into()),
            },
            30,
        );
        let db = HistoryDb::in_memory().unwrap();
        let path = std::env::temp_dir().join("pasteflow-e2e-report.json");
        let saved = export_sanitized_report(&c, Some(&db), path.to_str().unwrap()).unwrap();
        let json = std::fs::read_to_string(&saved).unwrap();
        // 脱敏铁律: 统计字段齐全, 绝无正文
        assert!(json.contains("generatedAt"));
        assert!(json.contains("historyTotal"));
        assert!(json.contains("rich_office"));
        assert!(json.contains("\"G1B\""));
        assert!(!json.contains("rawPlainText"));
        assert!(!json.contains("contentHash"));
        assert!(!json.contains("snippet"));
        let _ = std::fs::remove_file(&saved);
    }
}
