//! P1-15a 真机 Provider 绑定 — objc2 declare_class! NSPasteboardItemDataProvider
//!
//! 将 P1 纯逻辑引擎接到真实 NSPasteboard DataProvider:
//!   - `PasteFlowProvider` 实现协议方法 `pasteboard:item:provideDataForType:`
//!   - 回调内 O(1) 读 frontmost -> PipeExecutor 路由 (RichOffice/ImCleaned/Suppress)
//!   - CallbackBudget catch_unwind 保护 (R1: 8ms 预算, panic 不穿过 FFI)
//!   - 首读 Probe Guard + Re-own 补挂触发 (P0 exp5)
//!
//! 铁律:
//!   R1 回调 8ms 硬预算 + catch_unwind
//!   R2 双通道同步抑制 (SuppressRich 时 html/rtf 同步不供给)
//!   plainText 通道必须显式注册 (Master Spec §8.3 R1)

use crate::clipboard_owner::SharedOwner;
use crate::double_copy::DoubleCopyDetector;
use crate::filters::budget::run_with_budget;
use crate::pipe_executor::{is_probe_guard, ProbeGuardContext, PipeExecutor, RouteClass};
use crate::privacy_guard::DlpDetector;
use crate::window_observer::WindowObserver;
use objc2::msg_send_id;
use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{declare_class, mutability, ClassType, DeclaredClass};
use objc2_app_kit::{
    NSPasteboard, NSPasteboardItem, NSPasteboardItemDataProvider, NSPasteboardType,
    NSPasteboardTypeHTML, NSPasteboardTypeRTF, NSPasteboardTypeString, NSPasteboardWriting,
    NSWorkspace,
};
use objc2_foundation::{NSArray, NSString};
use std::sync::Arc;
use std::time::Instant;

/// Provider 依赖的引擎状态 (跨线程共享)
pub struct ProviderDeps {
    pub window_observer: Arc<WindowObserver>,
    pub pipe_executor: Arc<PipeExecutor>,
    pub clipboard_owner: SharedOwner,
    pub double_copy: Arc<DoubleCopyDetector>,
    pub dlp: Arc<DlpDetector>,
    /// 挂载时刻 (单调): 用于 Probe Guard 的 elapsed_ms 判定
    pub mounted_at: Instant,
    /// 挂载时前台 bundle (Probe Guard 比对 source_bundle)
    pub source_bundle: String,
    /// 当前补挂代数 (0 = 原始挂载)
    pub generation: usize,
    /// 挂载时缓存的原始 source 文本 (PipeExecutor 输入)
    pub source_text: String,
}

// 真实 NSPasteboardItemDataProvider 实现
declare_class!(
    struct PasteFlowProvider;

    // SAFETY:
    // - Super = NSObject 无子类化约束
    // - 内部可变 (回调从 pasteboard 服务器线程进入)
    // - 不实现 Drop
    unsafe impl ClassType for PasteFlowProvider {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "PasteFlowProvider";
    }

    impl DeclaredClass for PasteFlowProvider {
        type Ivars = Arc<ProviderDeps>;
    }

    unsafe impl NSObjectProtocol for PasteFlowProvider {}

    unsafe impl NSPasteboardItemDataProvider for PasteFlowProvider {
        #[method(pasteboard:item:provideDataForType:)]
        fn pasteboard_item_provide_data_for_type(&self, _pb: Option<&NSPasteboard>, item: &NSPasteboardItem, r#type: &NSPasteboardType) {
            let deps = self.ivars().clone();
            // R1: catch_unwind + 8ms 预算, panic 绝不穿过 FFI 边界
            let _ = run_with_budget(move || {
                provide_data(&deps, item, r#type);
                Some(())
            });
        }

        #[method(pasteboardFinishedWithDataProvider:)]
        fn pasteboard_finished_with_data_provider(&self, _pb: &NSPasteboard) {
            // 注意: 此回调内不得释放自身 (Retained 释放会 dealloc 正在执行的方法宿主)
            // Provider 保活引用由 engine 生命周期物化 (Materializer R4) 统一释放
            tracing::debug!(target: "pasteflow_lib::paste_provider", "pasteboardFinishedWithDataProvider");
        }
    }
);

// SAFETY: ivars 为 Arc<ProviderDeps> (Send+Sync), ObjC 对象本身跨线程仅被
// pasteboard 服务器回调触达; 允许将 Retained 持有权移交主线程保活容器
unsafe impl Send for PasteFlowProvider {}
unsafe impl Sync for PasteFlowProvider {}

impl PasteFlowProvider {
    /// 创建 Provider 实例 (持有引擎依赖)
    pub fn new(deps: Arc<ProviderDeps>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(deps);
        unsafe { msg_send_id![super(this), init] }
    }
}

/// 回调体: 按 type 供给数据 (R1 已被 run_with_budget 包裹)
fn provide_data(deps: &Arc<ProviderDeps>, item: &NSPasteboardItem, r#type: &NSPasteboardType) {
    let type_str = r#type.to_string();
    // 首读 Probe Guard: <150ms & frontmost == source = 嗅探者抢读 -> 原始数据
    let front = frontmost_bundle();
    let ctx = ProbeGuardContext {
        elapsed_ms: deps.mounted_at.elapsed().as_millis() as u64,
        frontmost_bundle: &front,
        source_bundle: &deps.source_bundle,
    };
    if is_probe_guard(&ctx) {
        let _ = set_string(item, &deps.source_text, &type_str);
        return;
    }

    // 目标应用路由 (O(1) 查表, TDD §3.4)
    let plan = deps
        .pipe_executor
        .route(&deps.source_text, &front, deps.generation);
    match (type_str.as_str(), plan.route) {
        // R2: SuppressRich / ImCleanedText 时 html/rtf 同步不供给 (nil)
        ("public.html", RouteClass::SuppressRich)
        | ("public.html", RouteClass::ImCleanedText)
        | ("public.rtf", RouteClass::SuppressRich)
        | ("public.rtf", RouteClass::ImCleanedText) => {}
        ("public.html", _) => {
            if let Some(html) = plan.html {
                let _ = set_string(item, &html, &type_str);
            }
        }
        ("public.rtf", _) => {
            if let Some(rtf) = plan.rtf {
                let _ = set_string(item, &rtf, &type_str);
            }
        }
        _ => {
            if let Some(plain) = plan.plain {
                let _ = set_string(item, &plain, &type_str);
            }
        }
    }
}

/// 当前前台应用 bundle (O(1), P0 实测 0.042µs)
pub fn frontmost_bundle() -> String {
    unsafe {
        NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .and_then(|app| app.bundleIdentifier())
            .map(|s| s.to_string())
            .unwrap_or_default()
    }
}

/// item.setString_forType 的安全包装
fn set_string(item: &NSPasteboardItem, s: &str, type_str: &str) -> bool {
    let ns = NSString::from_str(s);
    let t = NSString::from_str(type_str);
    unsafe { item.setString_forType(&ns, &t) }
}

/// 三通道类型常量 (R1: plainText 必须显式注册)
pub fn registered_types() -> Retained<NSArray<NSPasteboardType>> {
    // extern_static 为 &'static NSString; NSString 非 IsRetainable,
    // 用 retain 转 Retained 后 from_retained_slice
    fn retain_static(r#static: &'static NSPasteboardType) -> Retained<NSPasteboardType> {
        let ptr = r#static as *const NSPasteboardType as *mut NSPasteboardType;
        unsafe { Retained::retain(ptr) }.expect("pasteboard type static must be non-null")
    }
    NSArray::from_vec(vec![
        // SAFETY: 读取 AppKit 导出的 type 常量 (非空, 进程生命周期内有效)
        unsafe { retain_static(NSPasteboardTypeHTML) },
        unsafe { retain_static(NSPasteboardTypeRTF) },
        unsafe { retain_static(NSPasteboardTypeString) },
    ])
}

/// 挂载 Provider 到剪贴板 (真机 take_over 核心)
/// 返回本次写入的 changeCount (用于内部写入登记, 防 DoubleCopy 误判)
pub fn mount_provider(deps: Arc<ProviderDeps>) -> Option<i64> {
    unsafe {
        let pb = NSPasteboard::generalPasteboard();
        let _cleared = pb.clearContents();
        let item = NSPasteboardItem::new();
        let provider = PasteFlowProvider::new(deps.clone());
        let proto: Retained<ProtocolObject<dyn NSPasteboardItemDataProvider>> =
            ProtocolObject::from_retained(provider.clone());
        let types = registered_types();
        let ok = item.setDataProvider_forTypes(&proto, &types);
        if !ok {
            tracing::error!(target: "pasteflow_lib::paste_provider", "setDataProvider failed");
            return None;
        }
        // 保活: Provider 持有至 pasteboardFinishedWithDataProvider (由 Materializer 释放)
        deps.clipboard_owner
            .keep_provider(Box::new(provider), deps.generation);
        let items: Retained<NSArray<ProtocolObject<dyn NSPasteboardWriting>>> =
            NSArray::from_vec(vec![ProtocolObject::from_retained(item)]);
        let wrote = pb.writeObjects(&items);
        if !wrote {
            tracing::error!(target: "pasteflow_lib::paste_provider", "writeObjects failed");
            return None;
        }
        let cc = pb.changeCount() as i64;
        // 内部写入登记 (P0 §1.4.2: 防 DoubleCopyDetector 误判挂载/补挂为外部复制)
        deps.clipboard_owner.register_internal_write(cc);
        Some(cc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_types_has_three_channels() {
        let t = registered_types();
        assert_eq!(t.count(), 3);
    }

    #[test]
    fn provider_deps_holds_source_text() {
        let d = ProviderDeps {
            window_observer: Arc::new(WindowObserver::new()),
            pipe_executor: Arc::new(PipeExecutor::new(Arc::new(DlpDetector::new()))),
            clipboard_owner: Arc::new(crate::clipboard_owner::ClipboardOwner::new()),
            double_copy: Arc::new(DoubleCopyDetector::new()),
            dlp: Arc::new(DlpDetector::new()),
            mounted_at: Instant::now(),
            source_bundle: "com.pasteflow.app".into(),
            generation: 0,
            source_text: "# T".into(),
        };
        assert_eq!(d.source_text, "# T");
        assert_eq!(d.generation, 0);
    }
}
