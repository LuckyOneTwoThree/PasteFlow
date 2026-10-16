//! P1-4 PrivacyAndLoopGuard — Master Spec §3.2 / TDD §3.6
//!
//! 三层防线:
//!   1. 防死循环 Guard: changeCount 单调递增 + 3s 冷却 + L1 快照反向匹配
//!   2. 黑名单强制执行: 剪贴板历史接管/搜索/云同步 等能力对黑名单内容零供给
//!   3. 静默 DLP 三不保护: 高置信密钥不入历史 / 不进云端 / 不同步
//!
//! 铁律 (Master Spec §3.2): DLP 检测只做管控分流, 绝不修改用户粘贴明文

use parking_lot::Mutex;
use std::collections::HashSet;
use std::time::{Duration, Instant};

/// L1 快照哈希环形缓冲大小 (TDD §3.6)
const SNAPSHOT_RING: usize = 8;
/// 变更冷却窗口 (TDD §3.6: 3s 冷却抑制自我触发)
pub const COOLDOWN: Duration = Duration::from_secs(3);

// ——— DLP 高置信密钥检测 (预编译正则, 严禁 NSDataDetector, TDD §4.3) ———

/// 高置信密钥模式 (DLP_SILENT): OpenAI sk- / GitHub ghp_ / AWS AKIA / 私钥 PEM
/// 均为高度结构化、误报率极低的模式 (Master Spec DLP 策略表)
const DLP_PATTERNS: &[&str] = &[
    r"sk-[A-Za-z0-9]{20,}",           // OpenAI
    r"ghp_[A-Za-z0-9]{36}",           // GitHub PAT
    r"AKIA[0-9A-Z]{16}",              // AWS Access Key
    r"-----BEGIN [A-Z ]*PRIVATE KEY-----", // PEM 私钥
];

/// DLP 判定结果
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DlpAction {
    /// 低置信: 正常供给
    #[default]
    Normal,
    /// 高置信密钥: 静默三不 (不入历史 / 不进云端 / 不同步), 粘贴明文绝不修改
    SilentGuard,
}

pub struct DlpDetector {
    compiled: Vec<regex::Regex>,
}

impl Default for DlpDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl DlpDetector {
    /// 预编译全部正则 (P9 启动一次性初始化, 热路径零编译开销)
    pub fn new() -> Self {
        let compiled = DLP_PATTERNS
            .iter()
            .map(|p| {
                regex::Regex::new(p).unwrap_or_else(|e| panic!("DLP regex compile failed: {e}"))
            })
            .collect();
        Self { compiled }
    }

    /// 检测文本是否命中高置信密钥
    pub fn detect(&self, text: &str) -> DlpAction {
        if self.compiled.iter().any(|re| re.is_match(text)) {
            DlpAction::SilentGuard
        } else {
            DlpAction::Normal
        }
    }

    /// 显式脱敏 (⌃⌥D, Master Spec §4.5 / UI Spec §2.3):
    /// 将命中的密钥字面量替换为 `<REDACTED>`，返回 (脱敏后文本, 脱敏处数)。
    /// 仅供**用户显式触发**的脱敏粘贴通道使用；D8 静默三不链路绝不调用（不篡改粘贴明文）。
    pub fn redact(&self, text: &str) -> (String, usize) {
        let mut out = text.to_string();
        let mut hits = 0;
        for re in &self.compiled {
            let replaced = re.replace_all(&out, "<REDACTED>");
            hits += re.find_iter(&out).count();
            out = replaced.into_owned();
        }
        (out, hits)
    }
}

// ——— 防死循环 Guard ———

#[derive(Debug, Default)]
pub struct LoopGuardState {
    /// 最近 N 次 L1 快照哈希 (blake3 前 8 字节)
    snapshot_ring: Vec<u64>,
    /// 最近一次由 PasteFlow 自身写入的时间
    last_own_write: Option<Instant>,
    /// 最近一次 changeCount
    last_change_count: Option<i64>,
    /// PasteFlow 内部写入产生的 changeCount 登记 (Re-own 补挂 / Materializer 物化)
    recent_internal_writes: HashSet<i64>,
}

pub struct LoopGuard {
    state: Mutex<LoopGuardState>,
}

impl Default for LoopGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl LoopGuard {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(LoopGuardState::default()),
        }
    }

    /// 记录 L1 快照哈希; 返回 true = 疑似自我触发 (快照在 3s 内出现过 -> 反向匹配)
    pub fn register_snapshot(&self, snapshot_hash: u64) -> bool {
        let mut s = self.state.lock();
        let suspected = s.snapshot_ring.contains(&snapshot_hash);
        s.snapshot_ring.push(snapshot_hash);
        if s.snapshot_ring.len() > SNAPSHOT_RING {
            s.snapshot_ring.remove(0);
        }
        suspected
    }

    /// 记录 PasteFlow 自身写入 (Re-own 补挂 / 物化)
    pub fn register_own_write(&self, change_count: i64) {
        let mut s = self.state.lock();
        s.last_own_write = Some(Instant::now());
        s.recent_internal_writes.insert(change_count);
    }

    /// changeCount 是否为 PasteFlow 内部写入 (DoubleCopyDetector 短路判断, §1.4.2)
    pub fn is_internal_write(&self, change_count: i64) -> bool {
        self.state.lock().recent_internal_writes.contains(&change_count)
    }

    /// 清理过期内部写入登记 (changeCount 已远去)
    pub fn prune_internal_writes(&self, current_change_count: i64) {
        let mut s = self.state.lock();
        s.recent_internal_writes
            .retain(|&cc| cc >= current_change_count - 5);
    }

    /// 检测 changeCount 是否单调递增 + 3s 冷却
    /// 返回 true = 允许处理 (非自我触发), false = 抑制 (自我触发或冷却中)
    pub fn allow_change(&self, change_count: i64) -> bool {
        let mut s = self.state.lock();
        // 单调递增检查
        if let Some(last) = s.last_change_count {
            if change_count <= last {
                return false;
            }
        }
        // 3s 冷却: 自身写入后 3s 内的变更被抑制 (避免写入-被读-再触发循环)
        if let Some(t) = s.last_own_write {
            if t.elapsed() < COOLDOWN {
                return false;
            }
        }
        s.last_change_count = Some(change_count);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // —— DLP ——
    #[test]
    fn dlp_detects_openai_key() {
        let d = DlpDetector::new();
        assert_eq!(d.detect("token sk-abc123def456ghi789jkl012"), DlpAction::SilentGuard);
    }

    #[test]
    fn redact_replaces_secrets_and_counts_hits() {
        let d = DlpDetector::new();
        let (out, hits) = d.redact("key sk-abc123def456ghi789jkl012 end");
        assert!(out.contains("<REDACTED>"));
        assert!(!out.contains("sk-abc123def456ghi789jkl012"));
        assert_eq!(hits, 1);
    }

    #[test]
    fn redact_multiple_secret_kinds_in_one_pass() {
        let d = DlpDetector::new();
        let (out, hits) = d.redact("a sk-abc123def456ghi789jkl012 b ghp_abcdefghij0123456789klmnopqrstuvwxyz c");
        assert!(out.contains("a <REDACTED> b <REDACTED> c"));
        assert_eq!(hits, 2);
    }

    #[test]
    fn redact_clean_text_returns_unchanged() {
        let d = DlpDetector::new();
        let (out, hits) = d.redact("普通文本没有任何密钥。");
        assert_eq!(out, "普通文本没有任何密钥。");
        assert_eq!(hits, 0);
    }

    #[test]
    fn dlp_detects_github_pat() {
        let d = DlpDetector::new();
        assert_eq!(
            d.detect("ghp_abcdefghijklmnopqrstuvwxyz0123456789AB"),
            DlpAction::SilentGuard
        );
    }

    #[test]
    fn dlp_detects_aws_key() {
        let d = DlpDetector::new();
        assert_eq!(d.detect("AKIAIOSFODNN7EXAMPLE"), DlpAction::SilentGuard);
    }

    #[test]
    fn dlp_detects_pem_private_key() {
        let d = DlpDetector::new();
        assert_eq!(
            d.detect("-----BEGIN RSA PRIVATE KEY-----\nMIIEpA..."),
            DlpAction::SilentGuard
        );
    }

    #[test]
    fn dlp_passes_normal_text() {
        let d = DlpDetector::new();
        assert_eq!(d.detect("普通文本 hello world 12345"), DlpAction::Normal);
    }

    // —— LoopGuard ——
    #[test]
    fn loop_guard_snapshot_dedup() {
        let g = LoopGuard::new();
        assert!(!g.register_snapshot(111)); // 首次
        assert!(!g.register_snapshot(222));
        assert!(g.register_snapshot(111)); // 3s 内重复快照 = 疑似自我触发
    }

    #[test]
    fn loop_guard_monotonic_change_count() {
        let g = LoopGuard::new();
        assert!(g.allow_change(100));
        assert!(!g.allow_change(100)); // 非递增
        assert!(!g.allow_change(99));
    }

    #[test]
    fn loop_guard_cooldown_suppresses() {
        let g = LoopGuard::new();
        assert!(g.allow_change(1));
        g.register_own_write(2); // 自身写入
        assert!(!g.allow_change(3)); // 3s 冷却内被抑制
    }

    #[test]
    fn internal_write_tracking() {
        let g = LoopGuard::new();
        g.register_own_write(42);
        assert!(g.is_internal_write(42));
        assert!(!g.is_internal_write(43));
        g.prune_internal_writes(50);
        assert!(!g.is_internal_write(42)); // 已被清理
    }
}
