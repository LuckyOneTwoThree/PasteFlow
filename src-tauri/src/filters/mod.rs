//! L1 算子 — Master Spec §3.3 / TDD §3.4 (PipeExecutor 调用)
//!
//! 本地确定性规则, <5ms @64KB (TDD §7.4)。全部预编译, 零 NSDataDetector。
//! DLP 检测走 privacy_guard::DlpDetector (共享表驱动)。

pub mod budget;
pub mod markdown;
pub mod social_link_cleaner;
pub mod web_cleaner;
pub mod pdf_reflow;
pub mod dev_swiss;

pub use budget::{run_with_budget, BudgetOutcome, BUDGET};
pub use markdown::{markdown_to_html, markdown_to_rtf, markdown_to_im_text};
pub use social_link_cleaner::clean_social_text;
pub use web_cleaner::clean_web_text;
pub use pdf_reflow::reflow_pdf_text;
pub use dev_swiss::{detect_kind as detect_dev_kind, DevPayloadKind};

use crate::privacy_guard::DlpAction;

/// L1 算子输出: 给定 source 文本, 产出三通道 payload + DLP 判定
#[derive(Debug, Clone, Default)]
pub struct L1Output {
    pub html: Option<String>,
    pub rtf: Option<String>,
    pub plain: Option<String>,
    pub dlp: DlpAction,
}

/// L1 管道: Markdown -> 三通道 (html / rtf / im_text) + DLP 检测
/// 用于 S0-E2E 主链路验证 (D4 手动 ⌘C⌘C / 内联浮层单条 L1)
pub fn run_l1_pipeline(source_text: &str, dlp: &crate::privacy_guard::DlpDetector) -> L1Output {
    let action = dlp.detect(source_text);
    L1Output {
        html: Some(markdown_to_html(source_text)),
        rtf: Some(markdown_to_rtf(source_text)),
        plain: Some(markdown_to_im_text(source_text)),
        dlp: action,
    }
}
