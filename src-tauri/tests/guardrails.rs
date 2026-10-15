//! 测试门禁 2/3: guardrails — Dev Plan P1-14 / Master Spec §8.3 铁律
//!
//! 工程铁律守护门禁: 任何重构不得削弱以下不变量
//!   R1 Provider 回调 8ms 硬预算 + catch_unwind
//!   R2 双通道同步抑制 (html/rtf 同时 nil)
//!   R4 生命周期物化四步铁律 (退出/暂停先物化)
//!   G1A-G5 两阶段格式门控
//!   防死循环 (changeCount 单调 + 3s 冷却 + 快照反向匹配)
//!   DLP 静默三不 (绝不改写明文)
//!   TOCTOU 原子校验
//!   Re-own 补挂 (≤3 代 + 内部写入短路, P0 exp5)

use pasteflow_lib::clipboard_owner::{toctou_check, ClipboardOwner, ToctouResult};
use pasteflow_lib::double_copy::{CopyEventKind, DoubleCopyDetector};
use pasteflow_lib::filters::budget::{run_with_budget, BUDGET};
use pasteflow_lib::format_gate::{metadata_from_types, stage2, Stage1Verdict, VolumeBytes, VolumeTier};
use pasteflow_lib::materializer::{MaterializeReason, MaterializeRequest, MaterializeStep, Materializer};
use pasteflow_lib::pipe_executor::{ReownState, REOWN_MAX_GENERATIONS};
use pasteflow_lib::privacy_guard::{DlpAction, DlpDetector, LoopGuard};
use std::time::Duration;

// —— R1: CallbackBudget ——

#[test]
fn r1_callback_budget_is_8ms() {
    assert_eq!(BUDGET, Duration::from_millis(8));
}

#[test]
fn r1_panic_caught_never_escapes() {
    let out = run_with_budget(|| -> Option<i32> { panic!("ffi boundary") });
    assert!(out.panicked);
    assert_eq!(out.value, None); // 降级 None, 绝不向 FFI 抛
}

#[test]
fn r1_over_budget_flags_silently() {
    let out = run_with_budget(|| {
        std::thread::sleep(Duration::from_millis(10));
        Some(1)
    });
    assert!(out.over_budget); // 记 warning 而非 panic
}

// —— R2: 双通道同步抑制 ——

#[test]
fn r2_suppress_dual_channel_simultaneous() {
    use pasteflow_lib::pipe_executor::{PipeExecutor, RouteClass};
    use std::sync::Arc;
    let ex = PipeExecutor::new(Arc::new(DlpDetector::new()));
    for target in [
        "com.tinyspeck.slackmacgap",
        "com.microsoft.VSCode",
        "com.apple.Terminal",
    ] {
        let plan = ex.route("# src", target, 0);
        assert_eq!(plan.route, RouteClass::SuppressRich);
        assert!(plan.html.is_none() && plan.rtf.is_none()); // 同步抑制
    }
}

// —— R4: 生命周期物化四步 ——

#[test]
fn r4_materializer_four_step_sequence() {
    let m = Materializer::new();
    m.run_lifecycle(MaterializeRequest {
        reason: MaterializeReason::Quit,
        html: Some("<p>x</p>".into()),
        rtf: Some(r"{\rtf1 x}".into()),
        plain: Some("x".into()),
    });
    m.mark_providers_released();
    let log = m.sequence_log();
    assert_eq!(log[0], MaterializeStep::SignalCaptured);
    assert_eq!(log[1], MaterializeStep::MainThreadDispatched);
    assert_eq!(log[2], MaterializeStep::Materialized);
    assert_eq!(log.last().unwrap(), &MaterializeStep::ProvidersReleased);
}

#[test]
fn r4_quit_and_pause_trigger_materialize() {
    for reason in [MaterializeReason::Quit, MaterializeReason::Pause] {
        let m = Materializer::new();
        m.run_lifecycle(MaterializeRequest {
            reason,
            html: None,
            rtf: None,
            plain: Some("plain".into()),
        });
        assert_eq!(m.current_step(), MaterializeStep::Materialized);
    }
}

// —— G1A-G5 两阶段格式门控 ——

#[test]
fn gate_stage1_g1a_private_object() {
    let m = metadata_from_types(
        vec!["com.todesktop.draft".into()],
        vec!["com.todesktop.draft".into()],
        1,
    );
    assert_eq!(m.stage1(), Stage1Verdict::PrivateObject);
}

#[test]
fn gate_stage1_g1b_pure_image_but_word_tiff_passes() {
    let pure = metadata_from_types(
        vec!["public.tiff".into()],
        vec!["public.tiff".into()],
        1,
    );
    assert_eq!(pure.stage1(), Stage1Verdict::PureImage);
    let word = metadata_from_types(
        vec!["public.html".into(), "public.rtf".into(), "public.tiff".into()],
        vec!["public.html".into(), "public.rtf".into(), "public.tiff".into()],
        1,
    );
    assert_eq!(word.stage1(), Stage1Verdict::Pass);
}

#[test]
fn gate_stage1_g2_g3_g4() {
    let multi = metadata_from_types(vec!["public.utf8-plain-text".into()], vec!["public.utf8-plain-text".into()], 3);
    assert_eq!(multi.stage1(), Stage1Verdict::MultiItem);
    let file = metadata_from_types(vec!["public.file-url".into()], vec!["public.file-url".into()], 1);
    assert_eq!(file.stage1(), Stage1Verdict::FileCopy);
    let junk = metadata_from_types(
        vec!["CorePasteboardFlavorType".into()],
        vec!["CorePasteboardFlavorType".into()],
        1,
    );
    assert_eq!(junk.stage1(), Stage1Verdict::LegacyJunk);
}

#[test]
fn gate_stage2_volume_tiers() {
    assert_eq!(stage2(VolumeBytes { html: 1024, rtf: 0, plain: 0 }), VolumeTier::FullTakeover);
    assert_eq!(stage2(VolumeBytes { html: 100_000, rtf: 0, plain: 0 }), VolumeTier::LightPlainOnly);
    assert_eq!(stage2(VolumeBytes { html: 300_000, rtf: 0, plain: 0 }), VolumeTier::Refuse);
}

// —— 防死循环 ——

#[test]
fn loop_guard_monotonic_and_cooldown() {
    let g = LoopGuard::new();
    assert!(g.allow_change(1));
    assert!(!g.allow_change(1)); // 非递增
    g.register_own_write(2);
    assert!(!g.allow_change(3)); // 3s 冷却内抑制
}

#[test]
fn loop_guard_snapshot_dedup() {
    let g = LoopGuard::new();
    assert!(!g.register_snapshot(42));
    assert!(g.register_snapshot(42)); // 重复快照 = 自我触发嫌疑
}

// —— DLP 静默三不: 绝不改写明文 ——

#[test]
fn dlp_detects_but_never_mutates() {
    let d = DlpDetector::new();
    let input = "key sk-abc123def456ghi789jkl012mno here";
    assert_eq!(d.detect(input), DlpAction::SilentGuard);
    // detect 是纯判定: 不提供任何改写 API, 明文输入即明文输出语义 (契约级)
    // 此处通过仅调用 detect 验证无副作用路径
    assert_eq!(d.detect("普通文本"), DlpAction::Normal);
}

// —— TOCTOU 原子校验 ——

#[test]
fn toctou_cas_semantics() {
    assert_eq!(toctou_check(5, 5), ToctouResult::Safe);
    assert_eq!(toctou_check(5, 6), ToctouResult::Aborted);
}

// —— Re-own 补挂 (P0 exp5 / §1.4.2) ——

#[test]
fn reown_cap_three_generations() {
    let mut st = ReownState::default();
    assert!(!st.exhausted());
    // 允许补挂 REOWN_MAX_GENERATIONS 次 (0,1,2 代各一次)
    for _ in 0..REOWN_MAX_GENERATIONS {
        assert!(st.note_timeout());
        st.mark_triggered();
    }
    assert!(st.exhausted()); // 达上限降级静态供给
    assert!(!st.note_timeout()); // 超限不再补挂
}

#[test]
fn reown_triggers_only_after_all_channels_served() {
    let mut st = ReownState {
        declared_channels: 3,
        ..Default::default()
    };
    assert!(!st.note_served());
    assert!(!st.note_served());
    assert!(st.note_served()); // 3/3
}

#[test]
fn reown_timeout_forces_trigger() {
    let mut st = ReownState {
        declared_channels: 3,
        ..Default::default()
    };
    st.note_served(); // 1/3
    assert!(st.note_timeout()); // 300ms 超时兜底
}

// —— DoubleCopy: 补挂 changeCount 短路 ——

#[test]
fn double_copy_internal_write_short_circuit() {
    let d = DoubleCopyDetector::new();
    assert_eq!(d.on_copy(111, true), CopyEventKind::Normal);
    d.short_circuit_internal(); // Re-own 补挂
    assert_eq!(d.on_copy(222, true), CopyEventKind::Normal); // 不误判双击
}

#[test]
fn double_copy_debounce_260ms() {
    let d = DoubleCopyDetector::new();
    assert_eq!(d.on_copy(111, true), CopyEventKind::Normal);
    assert_eq!(d.on_copy(111, true), CopyEventKind::Debounced);
}

// —— ClipboardOwner: 内部写入登记 ——

#[test]
fn owner_internal_write_registration() {
    let o = ClipboardOwner::new();
    o.register_internal_write(42);
    assert!(o.is_internal_write(42));
    assert!(!o.is_internal_write(43));
}

#[test]
fn owner_generation_cap() {
    let o = ClipboardOwner::new();
    assert_eq!(o.next_generation(3), Some(1));
    assert_eq!(o.next_generation(3), Some(2));
    assert_eq!(o.next_generation(3), Some(3));
    assert_eq!(o.next_generation(3), None);
}
