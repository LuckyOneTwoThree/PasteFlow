//! P2-3 Micro-HUD 推荐动作生成 (IPC Spec v1.0 `HudActionOptionDto`)
//!
//! 双击 ⌘C⌘C 语义升级 (v0.1.5)：不再「直接还原原文」，而是 **Re-own + 唤醒 Micro-HUD**，
//! 并按当前上下文（前台应用白名单，§3.2）生成 1–4 槽位推荐动作；槽位 `0` 恒为
//! 「还原原文」（`builtin.restore-raw`）。
//!
//! 本模块只负责「推荐动作」纯逻辑；HUD 面板弹出与键盘状态机见 P2-1/P2-2。

use serde::{Deserialize, Serialize};

/// HUD 动作层级 (IPC Spec `HudActionOptionDto.tier`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ActionTier {
    L1Local,
    L2Vision,
    L3StreamingAi,
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
    let mut actions = Vec::with_capacity(3);

    if IM_WHITELIST.contains(&frontmost_bundle) {
        actions.push(HudActionOption {
            slot_key: "1".into(),
            pipe_id: "builtin.markdown-to-im-cleaned".into(),
            title: "AI 星号净化".into(),
            subtitle: Some("剥离 # 与 ** 、优化列表缩进".into()),
            tier: ActionTier::L1Local,
            icon: "sparkles".into(),
        });
    } else if OFFICE_WHITELIST.contains(&frontmost_bundle) {
        actions.push(HudActionOption {
            slot_key: "1".into(),
            pipe_id: "builtin.markdown-to-rich".into(),
            title: "富文本排版".into(),
            subtitle: Some("生成 Word/Notion 友好 HTML".into()),
            tier: ActionTier::L1Local,
            icon: "text-format".into(),
        });
    } else if DEV_WHITELIST.contains(&frontmost_bundle) {
        actions.push(HudActionOption {
            slot_key: "1".into(),
            pipe_id: "builtin.markdown-to-code".into(),
            title: "保留源码".into(),
            subtitle: Some("100% 原始 Markdown".into()),
            tier: ActionTier::L1Local,
            icon: "code".into(),
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
    });

    // 槽位 0: 恒为还原原文 (IPC Spec: 'builtin.restore-raw' 对应槽位 '0')
    actions.push(HudActionOption {
        slot_key: "0".into(),
        pipe_id: "builtin.restore-raw".into(),
        title: "还原原文".into(),
        subtitle: None,
        tier: ActionTier::L1Local,
        icon: "undo".into(),
    });

    actions
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
}
