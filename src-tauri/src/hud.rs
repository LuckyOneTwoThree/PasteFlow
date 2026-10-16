//! P2-3 Micro-HUD 推荐动作生成 (IPC Spec v1.0 `HudActionOptionDto`)
//!
//! 双击 ⌘C⌘C 语义升级 (v0.1.5)：不再「直接还原原文」，而是 **Re-own + 唤醒 Micro-HUD**，
//! 并按当前上下文（前台应用白名单，§3.2）生成 1–4 槽位推荐动作；槽位 `0` 恒为
//! 「还原原文」（`builtin.restore-raw`）。
//!
//! 本模块只负责「推荐动作」纯逻辑；HUD 面板弹出与键盘状态机见 P2-1/P2-2。

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// HUD 动作层级 (IPC Spec `HudActionOptionDto.tier`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ActionTier {
    L1Local,
    L2Vision,
    L3StreamingAi,
}

/// HUD 槽位的 L3 动作语义 (P3-2: 贪吃蛇智能汇总 / 得体打工人)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HudL3Slot {
    MeetingNotes,
    ComparisonTable,
    TodoList,
    PoliteTone,
}

/// 推荐动作 (IPC Spec `HudActionOptionDto`)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HudActionOption {
    /// '1' | '2' | '3' | '4' | '0'
    pub slot_key: String,
    /// e.g. 'builtin.restore-raw'
    pub pipe_id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    pub tier: ActionTier,
    pub icon: String,
    /// L3 槽位语义 (仅 tier == L3_STREAMING_AI 时有值, P3-2)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub l3_slot: Option<HudL3Slot>,
}

// —— §3.2 应用白名单 ——

/// IM 纯文本美化白名单 (微信/企业微信/钉钉/飞书)
const IM_WHITELIST: &[&str] = &[
    "com.tencent.xinWeChat",
    "com.tencent.WeWorkMac",
    "com.alibaba.DingTalkMac",
    "com.electron.lark",
];

/// 办公文档白名单 (Word/WPS/Pages/飞书文档/Notion)
const OFFICE_WHITELIST: &[&str] = &[
    "com.microsoft.Word",
    "com.kingsoft.wpsoffice.mac",
    "com.apple.iWork.Pages",
    "notion.id",
    "com.electron.lark",
];

/// 开发者工具白名单 (Terminal/iTerm2/VSCode/Cursor/Warp/Xcode)
const DEV_WHITELIST: &[&str] = &[
    "com.apple.Terminal",
    "com.googlecode.iterm2",
    "com.microsoft.VSCode",
    "com.todesktop.cursor",
    "dev.warp.Warp-Stable",
    "com.apple.dt.Xcode",
];

/// 双击 ⌘C⌘C 唤醒 HUD 时，按前台应用生成推荐动作
///   - 槽位 `0` 恒为「还原原文」
///   - 槽位 `1..` 依 IM / 办公 / 开发者白名单上下文推荐
pub fn recommend_actions(frontmost_bundle: &str) -> Vec<HudActionOption> {
    let mut actions = Vec::with_capacity(5);

    if IM_WHITELIST.contains(&frontmost_bundle) {
        actions.push(HudActionOption {
            slot_key: "1".into(),
            pipe_id: "builtin.markdown-to-im-cleaned".into(),
            title: "AI 星号净化".into(),
            subtitle: Some("剥离 # 与 ** 、优化列表缩进".into()),
            tier: ActionTier::L1Local,
            icon: "sparkles".into(),
            l3_slot: None,
        });
    } else if OFFICE_WHITELIST.contains(&frontmost_bundle) {
        actions.push(HudActionOption {
            slot_key: "1".into(),
            pipe_id: "builtin.markdown-to-rich".into(),
            title: "富文本排版".into(),
            subtitle: Some("生成 Word/Notion 友好 HTML".into()),
            tier: ActionTier::L1Local,
            icon: "text-format".into(),
            l3_slot: None,
        });
    } else if DEV_WHITELIST.contains(&frontmost_bundle) {
        actions.push(HudActionOption {
            slot_key: "1".into(),
            pipe_id: "builtin.markdown-to-code".into(),
            title: "保留源码".into(),
            subtitle: Some("100% 原始 Markdown".into()),
            tier: ActionTier::L1Local,
            icon: "code".into(),
            l3_slot: None,
        });
    }

    // 社媒链接净化 (通用, 槽位 2)
    actions.push(HudActionOption {
        slot_key: "2".into(),
        pipe_id: "builtin.social-link-clean".into(),
        title: "社媒链接净化".into(),
        subtitle: Some("剥离追踪参数".into()),
        tier: ActionTier::L1Local,
        icon: "link".into(),
        l3_slot: None,
    });

    // P3-2: L3 流式动作 (UI Spec §3.3 流式预览展开态)
    actions.push(HudActionOption {
        slot_key: "3".into(),
        pipe_id: "builtin.l3-polite-tone".into(),
        title: "得体打工人润色".into(),
        subtitle: Some("AI 委婉专业改写 (L3)".into()),
        tier: ActionTier::L3StreamingAi,
        icon: "briefcase".into(),
        l3_slot: Some(HudL3Slot::PoliteTone),
    });
    actions.push(HudActionOption {
        slot_key: "4".into(),
        pipe_id: "builtin.l3-meeting-notes".into(),
        title: "AI 智能汇总".into(),
        subtitle: Some("生成结构化纪要 (L3)".into()),
        tier: ActionTier::L3StreamingAi,
        icon: "sparkles".into(),
        l3_slot: Some(HudL3Slot::MeetingNotes),
    });

    // 槽位 0: 恒为还原原文 (IPC Spec: 'builtin.restore-raw' 对应槽位 '0')
    actions.push(HudActionOption {
        slot_key: "0".into(),
        pipe_id: "builtin.restore-raw".into(),
        title: "还原原文".into(),
        subtitle: None,
        tier: ActionTier::L1Local,
        icon: "undo".into(),
        l3_slot: None,
    });

    actions
}

// —— P2-1 收尾: HUD 唤醒协调器 (IPC Spec §4 `pasteflow://hud-summoned`) ——

/// HUD 唤醒请求 (DoubleCopyDetector 双击触发 -> Re-own + 唤醒 Micro-HUD)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HudSummonPayload {
    /// L1 快照哈希 (关联当前复制内容)
    pub snapshot_hash: u64,
    /// 推荐动作 (槽位 `0` 恒为还原原文)
    pub actions: Vec<HudActionOption>,
    /// 唤醒来源
    pub source: HudSummonSource,
    /// 前台应用 bundle id (HUD 顶部上下文栏)
    pub frontmost_bundle: String,
    /// 当前复制原文 (L3 流式输入源, P3-1)
    pub source_text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HudSummonSource {
    /// 双击 ⌘C⌘C (<250ms)
    DoubleCopy,
    /// 全局热键 ⌃⌥V
    GlobalHotkey,
}

/// 双击 ⌘C⌘C -> Re-own + 唤醒 Micro-HUD (Dev Plan P2-1/P2-3 收尾)
///
/// 返回 HUD 唤醒载荷供 `pasteflow://hud-summoned` 事件广播 + 面板 `orderFrontRegardless`。
/// Re-own 补挂由 `ClipboardOwner::next_generation` 在事件消费链路中执行（不在此重复）。
pub fn summon_on_double_copy(
    snapshot_hash: u64,
    frontmost_bundle: &str,
    source_text: &str,
) -> HudSummonPayload {
    HudSummonPayload {
        snapshot_hash,
        actions: recommend_actions(frontmost_bundle),
        source: HudSummonSource::DoubleCopy,
        frontmost_bundle: frontmost_bundle.to_string(),
        source_text: source_text.to_string(),
    }
}

/// 全局热键 ⌃⌥V 唤醒 (IPC Spec §4)
pub fn summon_on_hotkey(
    snapshot_hash: u64,
    frontmost_bundle: &str,
    source_text: &str,
) -> HudSummonPayload {
    HudSummonPayload {
        snapshot_hash,
        actions: recommend_actions(frontmost_bundle),
        source: HudSummonSource::GlobalHotkey,
        frontmost_bundle: frontmost_bundle.to_string(),
        source_text: source_text.to_string(),
    }
}

/// HUD 唤醒总线 (EngineState -> lib.rs 事件广播链路)
///
/// 双击/热键在引擎层发出唤醒载荷，由 lib.rs 轮询 drain 后
/// `emit_to("micro-hud", "pasteflow://hud-summoned", payload)` + 面板 `orderFrontRegardless`。
#[derive(Debug, Default)]
pub struct HudSummonBus {
    queue: std::sync::Mutex<VecDeque<HudSummonPayload>>,
}

impl HudSummonBus {
    pub fn new() -> Self {
        Self::default()
    }

    /// 发出唤醒请求 (线程安全)
    pub fn emit(&self, payload: HudSummonPayload) {
        self.queue.lock().unwrap().push_back(payload);
    }

    /// 取出全部待广播请求 (lib.rs 消费)
    pub fn drain(&self) -> Vec<HudSummonPayload> {
        self.queue.lock().unwrap().drain(..).collect()
    }

    pub fn pending_count(&self) -> usize {
        self.queue.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn im_context_recommends_cleaner() {
        let a = recommend_actions("com.tencent.xinWeChat");
        assert_eq!(a[0].pipe_id, "builtin.markdown-to-im-cleaned");
        assert_eq!(a[0].slot_key, "1");
    }

    #[test]
    fn office_context_recommends_rich() {
        let a = recommend_actions("com.microsoft.Word");
        assert_eq!(a[0].pipe_id, "builtin.markdown-to-rich");
    }

    #[test]
    fn dev_context_recommends_raw_source() {
        let a = recommend_actions("com.apple.Terminal");
        assert_eq!(a[0].pipe_id, "builtin.markdown-to-code");
    }

    #[test]
    fn unknown_context_has_no_slot1_pipe_but_restore_raw() {
        let a = recommend_actions("com.unknown.app");
        // 无 IM/办公/开发者上下文 -> 无槽位 1 专属推荐
        assert!(!a.iter().any(|x| x.slot_key == "1"));
        // 但还原原文恒在
        let restore = a.iter().find(|x| x.slot_key == "0").unwrap();
        assert_eq!(restore.pipe_id, "builtin.restore-raw");
    }

    #[test]
    fn restore_raw_always_slot_zero() {
        for bundle in ["com.tencent.xinWeChat", "com.microsoft.Word", "x"] {
            let a = recommend_actions(bundle);
            let restore = a.iter().find(|x| x.pipe_id == "builtin.restore-raw").unwrap();
            assert_eq!(restore.slot_key, "0");
        }
    }

    #[test]
    fn slots_are_unique() {
        let a = recommend_actions("com.tencent.xinWeChat");
        let mut keys: Vec<_> = a.iter().map(|x| x.slot_key.as_str()).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), a.len(), "slot_key must be unique");
    }

    #[test]
    fn summon_on_double_copy_builds_payload() {
        let p = summon_on_double_copy(42, "com.tencent.xinWeChat", "原文");
        assert_eq!(p.snapshot_hash, 42);
        assert_eq!(p.source, HudSummonSource::DoubleCopy);
        assert_eq!(p.source_text, "原文");
        assert_eq!(p.frontmost_bundle, "com.tencent.xinWeChat");
        assert!(p.actions.iter().any(|a| a.pipe_id == "builtin.restore-raw"));
    }

    #[test]
    fn summon_on_hotkey_marks_global_hotkey_source() {
        let p = summon_on_hotkey(7, "com.apple.Terminal", "src");
        assert_eq!(p.source, HudSummonSource::GlobalHotkey);
        assert_eq!(p.source_text, "src");
    }

    #[test]
    fn l3_slots_carry_semantics() {
        // P3-2: 槽位 3 得体打工人 / 槽位 4 智能汇总, 均为 L3 流式
        let a = recommend_actions("com.unknown.app");
        let t3 = a.iter().find(|x| x.slot_key == "3").unwrap();
        let t4 = a.iter().find(|x| x.slot_key == "4").unwrap();
        assert_eq!(t3.tier, ActionTier::L3StreamingAi);
        assert_eq!(t3.l3_slot, Some(HudL3Slot::PoliteTone));
        assert_eq!(t4.tier, ActionTier::L3StreamingAi);
        assert_eq!(t4.l3_slot, Some(HudL3Slot::MeetingNotes));
    }

    #[test]
    fn hud_summon_bus_queues_and_drains() {
        let bus = HudSummonBus::new();
        bus.emit(summon_on_double_copy(1, "com.unknown.a", "a"));
        bus.emit(summon_on_double_copy(2, "com.unknown.b", "b"));
        assert_eq!(bus.pending_count(), 2);
        let drained = bus.drain();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].snapshot_hash, 1);
        assert!(bus.drain().is_empty());
    }
}
