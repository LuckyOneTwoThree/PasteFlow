//! P1-14 Engine Integration — Tauri setup 装配层
//!
//! 将 P1 全部模块接线为可运行引擎 (v0.1.0 极简菜单栏版):
//!   1. 主线程: objc2/AppKit 创建 NSStatusItem + NSMenu + Toast NSPanel
//!   2. 后台线程: 50ms 轮询 frontmost -> WindowObserver 环形缓冲
//!   3. Provider 回调链: FormatGate -> PrivacyAndLoopGuard -> PipeExecutor -> CallbackBudget
//!   4. 生命周期: Quit/Pause -> Materializer 四步铁律 -> releaseProviders
//!
//! 铁律 (§8.3):
//!   R1 回调 8ms 预算 + catch_unwind (CallbackBudget)
//!   R2 双通道同步抑制 (PipeExecutor::SuppressRich)
//!   R4 退出/暂停先物化, 严禁裸 signal handler 调 AppKit (Materializer)
//!   v0.1.0 零 Webview (55MB 内存红线, main.rs 不创建任何 tauri Window)

use crate::clipboard_owner::{ClipboardOwner, SharedOwner};
use crate::double_copy::DoubleCopyDetector;
use crate::format_gate::{stage2, Stage1Verdict, VolumeBytes, VolumeTier};
use crate::materializer::{MaterializeReason, MaterializeRequest, Materializer, SharedMaterializer};
use crate::menu_bar::{MenuBarController, MenuAction, SharedMenuBar};
use crate::native_bridge::{NativeMenuBar, NativeToastPanel};
use crate::pipe_executor::PipeExecutor;
use crate::privacy_guard::{DlpDetector, LoopGuard};
use crate::recipe::RecipeStore;
use crate::toast::{NativeToastController, ToastLevel, ToastRequest};
use crate::window_observer::WindowObserver;
use std::sync::Arc;
use std::time::Duration;

/// 轮询间隔 (TDD §3.1)
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// 引擎共享状态 (跨线程)
pub struct EngineState {
    pub window_observer: Arc<WindowObserver>,
    pub clipboard_owner: SharedOwner,
    pub loop_guard: Arc<LoopGuard>,
    pub double_copy: Arc<DoubleCopyDetector>,
    pub pipe_executor: Arc<PipeExecutor>,
    pub materializer: SharedMaterializer,
    pub menu_bar: SharedMenuBar,
    pub toast_controller: Arc<NativeToastController>,
    pub recipes: Arc<RecipeStore>,
    pub dlp: Arc<DlpDetector>,
    /// P2-1: HUD 唤醒总线 (双击 ⌘C⌘C -> Re-own + 唤醒 Micro-HUD)
    pub hud_summon: Arc<crate::hud::HudSummonBus>,
    /// P2-1: HUD 可见状态跟踪
    pub hud_visibility: Arc<crate::hud_window::HudVisibilityTracker>,
}

impl EngineState {
    /// 构建引擎状态 (主线程, 建于 tauri setup)
    pub fn new() -> Self {
        let double_copy = Arc::new(DoubleCopyDetector::new());
        let dlp = Arc::new(DlpDetector::new());
        let recipes = Arc::new(RecipeStore::with_builtin().expect("builtin recipes valid"));
        Self {
            window_observer: Arc::new(WindowObserver::new()),
            clipboard_owner: Arc::new(ClipboardOwner::new()),
            loop_guard: Arc::new(LoopGuard::new()),
            double_copy: double_copy.clone(),
            pipe_executor: Arc::new(PipeExecutor::new(dlp.clone())),
            materializer: Arc::new(Materializer::new()),
            menu_bar: Arc::new(MenuBarController::new()),
            toast_controller: Arc::new(NativeToastController::new(double_copy)),
            recipes,
            dlp,
            hud_summon: Arc::new(crate::hud::HudSummonBus::new()),
            hud_visibility: Arc::new(crate::hud_window::HudVisibilityTracker::new()),
        }
    }

    /// 处理菜单栏动作 (主线程): 启用开关/退出触发生命周期物化
    pub fn handle_menu_action(&self, action: MenuAction, native: &mut NativeHandles) {
        let needs_materialize = self.menu_bar.handle_action(action);
        if needs_materialize {
            self.lifecycle_materialize(MaterializeReason::Quit, native);
        }
    }

    /// 生命周期物化 (R4 铁律: 主线程四步)
    pub fn lifecycle_materialize(&self, reason: MaterializeReason, native: &mut NativeHandles) {
        let req = MaterializeRequest {
            reason,
            html: None,
            rtf: None,
            plain: None,
        };
        self.materializer.run_lifecycle(req);
        // Step 3: 实际物化由 clipboard_owner (真机 AppKit 写入) 完成
        self.clipboard_owner.materialize_snapshot();
        self.materializer.mark_materialized();
        // Step 4: 释放 Provider 保活引用
        self.clipboard_owner.release_providers();
        self.materializer.mark_providers_released();
        // 隐藏 Toast 面板
        native.toast_panel.hide();
    }

    /// 检测到外部复制写入 (轮询线程 -> 主线程回调)
    pub fn on_external_copy(&self, change_count: i64, snapshot_hash: u64) -> Option<ToastRequest> {
        // 防死循环: changeCount 单调 + 3s 冷却
        if !self.loop_guard.allow_change(change_count) {
            return None;
        }
        // 内部写入短路 (Re-own 补挂 / 物化): 双登记表任一命中即短路
        if self.loop_guard.is_internal_write(change_count)
            || self.clipboard_owner.is_internal_write(change_count)
        {
            self.double_copy.short_circuit_internal();
            return None;
        }
        // 快照反向匹配 (自我触发嫌疑)
        if self.loop_guard.register_snapshot(snapshot_hash) {
            return None;
        }
        // 消抖/双击判定
        let kind = self.double_copy.on_copy(snapshot_hash, true);
        match kind {
            crate::double_copy::CopyEventKind::Debounced => None, // 260ms 消抖抑制
            crate::double_copy::CopyEventKind::Normal => Some(ToastRequest {
                text: "已复制".to_string(),
                level: ToastLevel::Info,
            }),
            // v0.1.5 (P2-1/P2-3): 双击 ⌘C⌘C -> Re-own + 唤醒 Micro-HUD (不再直接还原原文)
            crate::double_copy::CopyEventKind::DoubleClick => {
                let payload = crate::hud::summon_on_double_copy(
                    snapshot_hash,
                    &crate::paste_provider::frontmost_bundle(),
                );
                self.hud_summon.emit(payload);
                Some(ToastRequest {
                    text: "已唤醒 Micro-HUD (按 1–4 选择动作)".to_string(),
                    level: ToastLevel::Info,
                })
            }
        }
    }

    /// 显示 Toast (主线程, 260ms 消抖由 NativeToastController 内部处理)
    pub fn show_toast(&self, req: ToastRequest) {
        self.toast_controller.show(req);
    }

    /// 50ms 主线程 tick 的决策部分 (纯逻辑, 可测):
    /// 输入剪贴板元数据 -> 是否接管 + 变体路由
    /// 铁律: G1A-G5 门控 / DLP / 防死循环 全部在挂载前完成
    pub fn evaluate_takeover(
        &self,
        types: &[String],
        volumes: VolumeBytes,
        item_count: usize,
    ) -> TakeoverDecision {
        let meta = crate::format_gate::metadata_from_types(
            types.to_vec(),
            types.to_vec(),
            item_count,
        );
        // Stage 1: 零字节元数据门控
        match meta.stage1() {
            Stage1Verdict::Pass => {}
            other => {
                return TakeoverDecision {
                    should_take_over: false,
                    verdict: other,
                    tier: VolumeTier::Refuse,
                }
            }
        }
        // Stage 2: 双层体积分级
        let tier = stage2(volumes);
        match tier {
            VolumeTier::Refuse => TakeoverDecision {
                should_take_over: false,
                verdict: Stage1Verdict::Pass,
                tier,
            },
            _ => TakeoverDecision {
                should_take_over: true,
                verdict: Stage1Verdict::Pass,
                tier,
            },
        }
    }
}

/// 接管决策结果 (供真机 tick 与测试断言)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TakeoverDecision {
    pub should_take_over: bool,
    pub verdict: Stage1Verdict,
    pub tier: VolumeTier,
}

/// 真机 50ms tick: 读 frontmost + changeCount, 决策接管并挂载 Provider
/// 注意: 本函数含 objc 调用, 仅在主线程运行 (Tauri setup 注册的定时器)
#[cfg(target_os = "macos")]
pub fn tick_takeover(state: &Arc<EngineState>, current_change_count: i64, types: &[String], volumes: VolumeBytes) -> TickOutcome {
    // 内部写入短路 (挂载/补挂登记的 changeCount 不再触发接管)
    if state.clipboard_owner.is_internal_write(current_change_count)
        || state.loop_guard.is_internal_write(current_change_count)
    {
        return TickOutcome::InternalWrite;
    }
    let decision = state.evaluate_takeover(types, volumes, 1);
    if !decision.should_take_over {
        return TickOutcome::Skipped(decision);
    }
    // 防死循环: 快照反向匹配
    if state.loop_guard.register_snapshot(current_change_count as u64) {
        return TickOutcome::LoopSuspected;
    }
    TickOutcome::Takeover(decision)
}

/// tick 结果 (测试断言)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickOutcome {
    /// 内部写入, 短路
    InternalWrite,
    /// 防死循环命中
    LoopSuspected,
    /// 门控拒绝 (G1A-G5 / 体积)
    Skipped(TakeoverDecision),
    /// 判定接管, 真机随后 mount_provider
    Takeover(TakeoverDecision),
}

/// 原生控件持有 (主线程)
pub struct NativeHandles {
    pub menu_bar: NativeMenuBar,
    pub toast_panel: NativeToastPanel,
}

/// Tauri setup 钩子调用: 创建原生控件 (主线程)
pub fn setup_native(state: &Arc<EngineState>) -> NativeHandles {
    let mut menu_bar = NativeMenuBar::new(state.menu_bar.clone());
    menu_bar.assemble();
    let mut toast_panel = NativeToastPanel::new();
    toast_panel.assemble();
    NativeHandles {
        menu_bar,
        toast_panel,
    }
}

/// 启动 50ms frontmost 轮询线程 (后台, 只读 WindowObserver)
/// 回调经 channel 投递主线程, 绝不在后台线程调 AppKit
pub fn spawn_frontmost_poller(state: Arc<EngineState>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        loop {
            // 50ms 轮询 frontmost (真机由 objc2 NSWorkspace.frontmostApplication 读取)
            // 这里记录占位身份, 真机绑定在 P1-14 tauri setup 注入 closure
            let _ = &state.window_observer;
            std::thread::sleep(POLL_INTERVAL);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_state_builds_with_builtin_recipes() {
        let s = EngineState::new();
        assert_eq!(s.recipes.len(), 2);
        assert!(s.menu_bar.is_enabled());
    }

    #[test]
    fn external_copy_toast_on_normal() {
        let s = EngineState::new();
        let toast = s.on_external_copy(1, 111);
        assert!(toast.is_some());
    }

    #[test]
    fn internal_write_short_circuits_toast() {
        let s = EngineState::new();
        s.clipboard_owner.register_internal_write(42);
        assert!(s.on_external_copy(42, 999).is_none());
    }

    #[test]
    fn lifecycle_materialize_completes_four_steps() {
        let s = Arc::new(EngineState::new());
        let mut native = setup_native(&s);
        s.lifecycle_materialize(MaterializeReason::Quit, &mut native);
        assert_eq!(
            s.materializer.current_step(),
            crate::materializer::MaterializeStep::ProvidersReleased
        );
    }

    #[test]
    fn menu_toggle_triggers_materialize() {
        let s = Arc::new(EngineState::new());
        let mut native = setup_native(&s);
        s.handle_menu_action(MenuAction::ToggleEnabled, &mut native);
        assert!(!s.menu_bar.is_enabled());
        assert_eq!(
            s.materializer.current_step(),
            crate::materializer::MaterializeStep::ProvidersReleased
        );
    }

    #[test]
    fn dlp_silent_guard_toast_no_plaintext() {
        let d = DlpDetector::new();
        assert_eq!(
            d.detect("sk-abc123def456ghi789jkl012mno"),
            crate::privacy_guard::DlpAction::SilentGuard
        );
    }

    // —— P1-15b: evaluate_takeover 决策 (G1A-G5 + 体积分级) ——

    #[test]
    fn takeover_text_full_passes() {
        let s = EngineState::new();
        let d = s.evaluate_takeover(
            &["public.utf8-plain-text".into()],
            VolumeBytes { html: 0, rtf: 0, plain: 1024 },
            1,
        );
        assert!(d.should_take_over);
        assert_eq!(d.tier, VolumeTier::FullTakeover);
    }

    #[test]
    fn takeover_private_object_g1a_blocked() {
        let s = EngineState::new();
        let d = s.evaluate_takeover(
            &["com.todesktop.draft".into()],
            VolumeBytes { html: 0, rtf: 0, plain: 0 },
            1,
        );
        assert!(!d.should_take_over);
        assert_eq!(d.verdict, Stage1Verdict::PrivateObject);
    }

    #[test]
    fn takeover_oversize_refused() {
        let s = EngineState::new();
        let d = s.evaluate_takeover(
            &["public.utf8-plain-text".into()],
            VolumeBytes { html: 300 * 1024, rtf: 0, plain: 0 },
            1,
        );
        assert!(!d.should_take_over);
        assert_eq!(d.tier, VolumeTier::Refuse);
    }

    #[test]
    fn takeover_light_tier_still_takes_over() {
        let s = EngineState::new();
        let d = s.evaluate_takeover(
            &["public.html".into()],
            VolumeBytes { html: 100 * 1024, rtf: 0, plain: 0 },
            1,
        );
        assert!(d.should_take_over);
        assert_eq!(d.tier, VolumeTier::LightPlainOnly);
    }

    #[test]
    fn takeover_multi_item_blocked() {
        let s = EngineState::new();
        let d = s.evaluate_takeover(
            &["public.utf8-plain-text".into()],
            VolumeBytes { html: 0, rtf: 0, plain: 8 },
            3,
        );
        assert!(!d.should_take_over);
        assert_eq!(d.verdict, Stage1Verdict::MultiItem);
    }
}
