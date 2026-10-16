//! P2-2 Micro-HUD 键盘/鼠标状态机 (UI Spec v1.0 §3.2)
//!
//! 纯逻辑状态机：解析 HUD 显示期间的按键，产出「动作意图」。
//! **按键绝不泄漏进宿主应用**（由 NSPanel canBecomeKey + 事件拦截保证，见 hud_panel）。
//!
//! 按键映射 (UI Spec §3.2)：
//!   `1`–`4`         -> 触发对应编号动作并上屏
//!   `0` / Backspace -> 粘贴原始未转换内容
//!   `↑` / `↓` / Tab -> 循环移动高亮行（默认高亮第 1 项）
//!   `Enter`         -> 执行当前高亮行动作
//!   `Esc`           -> 隐藏 HUD（自动挡接管状态不变）

use serde::{Deserialize, Serialize};

/// HUD 可见状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HudVisibility {
    /// 常驻隐藏 (P2-1: 启动 2s 预热后 invisible 常驻)
    Hidden,
    /// 已唤醒 (双击 ⌘C⌘C / 全局热键)
    Summoned,
    /// Tier 0 上屏反馈: 「已准备就绪，请按 ⌘V」600ms
    ReadyToPaste,
    /// Tier 1 上屏反馈: 80ms 缩小淡出
    Injecting,
}

/// HUD 按键意图
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HudIntent {
    /// 触发指定槽位动作 (slot_key: '0'..'4')
    TriggerSlot(String),
    /// 移动高亮行 (delta: +1 / -1)
    MoveHighlight(i32),
    /// 执行当前高亮行
    Confirm,
    /// 隐藏 HUD
    Dismiss,
    /// 无动作 (不支持的键)
    None,
}

/// Tier 层级 (决定上屏反馈路径)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TierMode {
    /// 零权限写回: HUD 变形绿色确认胶囊 600ms
    Tier0,
    /// Accessibility 自动注入: 80ms 缩小淡出 + 自动 ⌘V
    Tier1,
}

/// HUD 状态机
#[derive(Debug, Clone)]
pub struct HudState {
    pub visibility: HudVisibility,
    /// 高亮行索引 (0-based, 对应 slot 1..N)
    pub highlight: usize,
    /// 可选动作槽数 (不含槽位 0 保底项)
    pub slot_count: usize,
    pub tier: TierMode,
}

impl HudState {
    /// 常驻隐藏态
    pub fn new(slot_count: usize, tier: TierMode) -> Self {
        Self {
            visibility: HudVisibility::Hidden,
            highlight: 0,
            slot_count: slot_count.max(1),
            tier,
        }
    }

    /// 双击 ⌘C⌘C / 热键唤醒
    pub fn summon(&mut self) {
        self.visibility = HudVisibility::Summoned;
        self.highlight = 0; // 默认高亮第 1 项 (UI Spec §3.2)
    }

    /// 解析按键 -> 动作意图
    pub fn handle_key(&mut self, key: &str) -> HudIntent {
        if self.visibility == HudVisibility::Hidden {
            return HudIntent::None;
        }
        match key {
            // 直接触发指定槽位
            "1" | "2" | "3" | "4" => {
                let idx = key.parse::<usize>().unwrap_or(1);
                if idx <= self.slot_count {
                    self.trigger_feedback();
                    HudIntent::TriggerSlot(key.to_string())
                } else {
                    HudIntent::None
                }
            }
            "0" | "Backspace" => {
                self.trigger_feedback();
                HudIntent::TriggerSlot("0".into())
            }
            // 高亮移动
            "ArrowUp" => self.move_highlight(-1),
            "ArrowDown" | "Tab" => self.move_highlight(1),
            "Enter" => {
                self.trigger_feedback();
                HudIntent::Confirm
            }
            "Escape" => {
                self.visibility = HudVisibility::Hidden;
                HudIntent::Dismiss
            }
            _ => HudIntent::None,
        }
    }

    /// 当前高亮行对应的槽位 (1-based)
    pub fn highlighted_slot(&self) -> String {
        (self.highlight + 1).to_string()
    }

    fn move_highlight(&mut self, delta: i32) -> HudIntent {
        let n = self.slot_count as i32;
        let next = (self.highlight as i32 + delta).rem_euclid(n);
        self.highlight = next as usize;
        HudIntent::MoveHighlight(delta)
    }

    /// 上屏反馈 (UI Spec §3.2)
    fn trigger_feedback(&mut self) {
        self.visibility = match self.tier {
            TierMode::Tier0 => HudVisibility::ReadyToPaste,
            TierMode::Tier1 => HudVisibility::Injecting,
        };
    }

    /// 反馈动画结束 (600ms Tier0 / 80ms Tier1) 后回隐藏
    pub fn finish_feedback(&mut self) {
        if matches!(
            self.visibility,
            HudVisibility::ReadyToPaste | HudVisibility::Injecting
        ) {
            self.visibility = HudVisibility::Hidden;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_state_ignores_keys() {
        let mut h = HudState::new(3, TierMode::Tier0);
        assert_eq!(h.handle_key("1"), HudIntent::None);
        assert_eq!(h.handle_key("Escape"), HudIntent::None);
    }

    #[test]
    fn summon_defaults_highlight_first() {
        let mut h = HudState::new(3, TierMode::Tier0);
        h.summon();
        assert_eq!(h.visibility, HudVisibility::Summoned);
        assert_eq!(h.highlight, 0);
        assert_eq!(h.highlighted_slot(), "1");
    }

    #[test]
    fn digit_triggers_slot_and_tier0_feedback() {
        let mut h = HudState::new(3, TierMode::Tier0);
        h.summon();
        assert_eq!(h.handle_key("2"), HudIntent::TriggerSlot("2".into()));
        assert_eq!(h.visibility, HudVisibility::ReadyToPaste);
    }

    #[test]
    fn digit_triggers_slot_and_tier1_feedback() {
        let mut h = HudState::new(3, TierMode::Tier1);
        h.summon();
        assert_eq!(h.handle_key("1"), HudIntent::TriggerSlot("1".into()));
        assert_eq!(h.visibility, HudVisibility::Injecting);
    }

    #[test]
    fn out_of_range_slot_ignored() {
        let mut h = HudState::new(2, TierMode::Tier0);
        h.summon();
        assert_eq!(h.handle_key("3"), HudIntent::None);
    }

    #[test]
    fn zero_and_backspace_restore_raw() {
        for key in ["0", "Backspace"] {
            let mut h = HudState::new(3, TierMode::Tier0);
            h.summon();
            assert_eq!(h.handle_key(key), HudIntent::TriggerSlot("0".into()));
        }
    }

    #[test]
    fn arrows_cycle_highlight() {
        let mut h = HudState::new(3, TierMode::Tier0);
        h.summon();
        assert_eq!(h.handle_key("ArrowDown"), HudIntent::MoveHighlight(1));
        assert_eq!(h.highlight, 1);
        assert_eq!(h.handle_key("ArrowUp"), HudIntent::MoveHighlight(-1));
        assert_eq!(h.highlight, 0);
        // 循环: 向上到顶再上 -> 环绕到最后一行
        assert_eq!(h.handle_key("ArrowUp"), HudIntent::MoveHighlight(-1));
        assert_eq!(h.highlight, 2);
    }

    #[test]
    fn tab_cycles_highlight() {
        let mut h = HudState::new(3, TierMode::Tier0);
        h.summon();
        h.handle_key("Tab");
        assert_eq!(h.highlight, 1);
    }

    #[test]
    fn enter_confirms_highlighted() {
        let mut h = HudState::new(3, TierMode::Tier0);
        h.summon();
        h.handle_key("ArrowDown"); // 高亮第 2 项
        assert_eq!(h.handle_key("Enter"), HudIntent::Confirm);
        assert_eq!(h.highlighted_slot(), "2");
    }

    #[test]
    fn escape_hides_keeps_auto_mode() {
        let mut h = HudState::new(3, TierMode::Tier0);
        h.summon();
        assert_eq!(h.handle_key("Escape"), HudIntent::Dismiss);
        assert_eq!(h.visibility, HudVisibility::Hidden);
    }

    #[test]
    fn unsupported_key_is_none() {
        let mut h = HudState::new(3, TierMode::Tier0);
        h.summon();
        assert_eq!(h.handle_key("x"), HudIntent::None);
    }

    #[test]
    fn finish_feedback_returns_to_hidden() {
        let mut h = HudState::new(3, TierMode::Tier0);
        h.summon();
        h.handle_key("1");
        assert_eq!(h.visibility, HudVisibility::ReadyToPaste);
        h.finish_feedback();
        assert_eq!(h.visibility, HudVisibility::Hidden);
    }
}
