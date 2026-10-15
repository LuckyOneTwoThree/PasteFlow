# PasteFlow (灵贴) — Rust 核心引擎技术设计文档 (TDD v1.0)

> **文档状态：** 生效中（Aligned with Master Spec v1.4 & Step 0A Benchmark）
> **修订日期：** 2026-10-15
> **上游真源：** [[PasteFlow (灵贴) — 产品定义与架构源文档 (Master Spec v1.4)]]
> **关联文档：** [[PasteFlow (灵贴) — UI与交互设计规范 (UI Spec v1.0)]] · [[PasteFlow (灵贴) — 前端状态与 Tauri IPC 接口契约 (IPC Spec v1.0)]]

## 1. 架构概览与工程目录结构 (Crate Layout & Module Tree)

本设计文档定义 PasteFlow 后端 Rust 核心引擎（`pasteflow-core` 与 `src-tauri`）的模块边界、Trait 接口契约、跨平台 FFI 内存安全模型、线程并发模型以及 SQLite 存储 Schema。

### 1.1 目录结构规范

```text
pasteflow/
├── Cargo.toml
├── pipes/                                  # 内置声明式 PipeRecipe JSON 配方目录
│   ├── builtin.markdown-to-office.json
│   └── builtin.social-link-cleaner.json
├── schema/
│   └── pipe-v1.json                        # PipeRecipe JSON Schema (additionalProperties: false)
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   └── src/
│       ├── main.rs                         # 进程入口、Panic Hook 与 GCD 主线程信号源挂载
│       ├── lib.rs                          # Tauri AppBuilder、生命周期钩子与模块装配
│       ├── core/
│       │   ├── mod.rs
│       │   ├── hotkeys.rs                  # 全局唯一快捷键常量登记表 (§4.5, 含 ⌘⇧P 与 ⌃⌥D)
│       │   ├── types.rs                    # ClipboardItem / PipeRecipe / FormatGateResult 核心类型
│       │   ├── loop_privacy_guard.rs       # 模块 8: PrivacyAndLoopGuard (防死循环 + 黑名单 + 静默 DLP)
│       │   ├── format_gate.rs              # 模块 2: FormatGate (Stage1 G1A/G1B/G2/G3/G4 零读取 + Stage2 G5 64KB/256KB)
│       │   ├── window_observer.rs          # 模块 3: WindowObserver (16槽 Arc 激活缓冲 + O(1) FrontmostApp)
│       │   ├── double_copy_detector.rs     # 模块 7: DoubleCopyDetector (⌘C⌘C 差分检测 + 260ms 消抖协同)
│       │   ├── pipe_executor.rs            # 模块 4: PipeExecutor (L1 转换路由 + DYNAMIC_BY_TARGET + HTML/RTF 双抑制)
│       │   ├── callback_budget.rs          # 模块 5: CallbackBudget (FFI catch_unwind + 8ms 预算监控)
│       │   ├── clipboard_owner.rs          # 模块 1: ClipboardOwner (50ms 轮询 + CAS 接管 + DataProvider)
│       │   └── materializer.rs             # 模块 6: Materializer (退出/暂停前所有权校验与主线程静态物化)
│       ├── handlers/                       # L1 确定性转换算子 (纯内存计算，零 I/O)
│       │   ├── mod.rs
│       │   ├── markdown_html.rs            # Markdown -> 带 mso- 样式的 Word/飞书兼容 CF_HTML
│       │   ├── markdown_rtf.rs             # Markdown -> 标准 RTF 语法树生成
│       │   ├── markdown_im_text.rs         # Markdown -> 单遍 UTF-8 字节流微信/钉钉/飞书 IM 去星号纯文本
│       │   ├── social_link_cleaner.rs      # 预编译正则 URL 提取 + 电商/社媒口令与域名分级参数剥离
│       │   └── dlp_detector.rs             # 高置信度密钥识别与显式 <REDACTED> 替换
│       ├── platform/
│       │   ├── mod.rs
│       │   ├── macos/
│       │   │   ├── pasteboard_provider.rs  # objc2 NSPasteboardItemDataProvider 实现与强引用保活
│       │   │   ├── workspace_observer.rs   # NSWorkspace.didActivateApplicationNotification 桥接
│       │   │   ├── non_activating_panel.rs # NonActivatingHUDPanel (NSPanel 子类, canBecomeKey=true)
│       │   │   ├── native_toast.rs         # NativeToastController (v0.1.0/v0.1.5 原生无焦点胶囊 Toast)
│       │   │   └── caret_ax.rs             # Tier 1 AXUIElement 光标定位 + 鼠标坐标三级回退
│       │   └── windows/                    # v0.2.0 实现 (OleSetClipboard + IDataObject + WinEventHook)
│       ├── storage/
│       │   ├── mod.rs
│       │   ├── db.rs                       # SQLite3 连接池、WAL 模式与 FTS5 (trigram) 全文检索
│       │   └── metrics.rs                  # 本地诊断面板统计指标聚合与脱敏周报导出
│       └── ipc/
│           ├── mod.rs
│           ├── commands.rs                 # Tauri Commands 处理器
│           └── events.rs                   # Tauri Events 广播发射器
└── vendor/
    └── clipboard-rs/                       # Vendor 锁定的底层剪贴板辅助库
```

### 1.2 `Cargo.toml` 依赖规范

```toml
[package]
name = "pasteflow"
version = "0.1.0"
edition = "2021"
license = "Apache-2.0"

[dependencies]
tauri = { version = "2.1", features = ["tray-icon", "macos-private-api"] }
tauri-plugin-global-shortcut = "2.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1.40", features = ["rt-multi-thread", "sync", "time", "macros", "signal"] }
parking_lot = "0.12"
blake3 = "1.5"
uuid = { version = "1.10", features = ["v7", "serde"] }
regex = "1.11"                              # 线性时间 DFA/NFA，天然免疫灾难性回溯
pulldown-cmark = { version = "0.12", default-features = false }
scraper = "0.20"
url = "2.5"
rusqlite = { version = "0.40", features = ["bundled", "fts5"] }
active-win-pos-rs = "0.11"
enigo = "0.6"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }

[target.'cfg(target_os = "macos")'.dependencies]
objc2 = "0.5"
objc2-foundation = { version = "0.2", features = ["NSArray", "NSData", "NSString", "NSDictionary"] }
objc2-app-kit = { version = "0.2", features = [
    "NSPasteboard",
    "NSPasteboardItem",
    "NSWorkspace",
    "NSRunningApplication",
    "NSPanel",
    "NSWindow",
    "NSEvent",
    "NSApplication"
] }
libc = "0.2"
```

## 2. 线程与并发模型 (Threading & Concurrency Model)

由于 macOS `AppKit` 对主线程（Main RunLoop）有严格亲和性要求，系统严格划分以下三类执行上下文，**严禁在主线程等待异步 Tokio 锁，严禁在裸 POSIX Signal Handler 内直接调用 Objective-C `NSPasteboard`**：

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ Thread 1: macOS AppKit Main Thread (RunLoop)                                 │
│  • 职责 1: 响应 NSPasteboardItemDataProvider::provideDataForType 同步回调    │
│  • 职责 2: 接收 NSWorkspace.didActivateApplicationNotification 通知          │
│  • 职责 3: 管理 NSStatusItem、NativeToastController 与 NonActivatingHUDPanel │
│  • 职责 4: 响应 GCD Signal Source (SIGTERM/SIGINT) 与退出前主线程同步物化     │
│  • 铁律: 只允许获取 parking_lot 短临界区同步锁 (<1µs)，严禁阻塞等待磁盘/网络 │
└───────────────┬──────────────────────────────────────────────▲───────────────┘
                │ 异步事件发送 (tokio::sync::mpsc::unbounded_send) │ O(1) Arc 克隆
                ▼                                              │ (WindowObserver)
┌──────────────────────────────────────────────────────────────┴───────────────┐
│ Thread Pool 2: Clipboard Watcher Dedicated Thread (50ms Ticker)              │
│  • 每 50ms 读取 NSPasteboard.general.changeCount (仅读取共享内存整型)        │
│  • 检测到外部自增时: 执行 PrivacyAndLoopGuard -> FormatGate Stage 1 (0读取)  │
│    -> 读取 UTF-8 文本 -> FormatGate Stage 2 (64KB/256KB 分级)                │
│    -> DoubleCopyDetector -> 规则匹配 -> 原子 CAS 校验 -> 注册 DataProvider   │
└───────────────┬──────────────────────────────────────────────────────────────┘
                │ 异步任务派发
                ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ Thread Pool 3: Tokio Background Worker Runtime                               │
│  • 职责 1: SQLite FTS5 (trigram) 异步落盘 (original 入库、指标更新)          │
│  • 职责 2: 260ms 自动挡消抖 Toast 计时器调度                                 │
│  • 职责 3: v0.1.5 L3 流式大模型 HTTP SSE 请求与 v0.2.0 L2 Vision OCR 预热    │
└──────────────────────────────────────────────────────────────────────────────┘
```

## 3. 八大核心模块接口与实现规范 (Core Modules Specification)

### 3.1 模块 1：`ClipboardOwner`（剪贴板持有、原子接管与延迟供给）

#### 核心职责
1. 以 `50ms`（20Hz）周期轮询系统 `changeCount`。
2. 在 macOS 15.4+ 上支持通过 `accessBehavior` 检查权限状态，并在接管前最后一行执行 **TOCTOU 原子校验**。
3. 持有当前激活的 Objective-C `PasteboardDataProvider` 强引用（`Retained<PasteboardDataProvider>`），防止引用计数归零导致回调悬空。
4. 当用户暂停自动挡（`cmd_set_autopilot_enabled(false)`）且当前持有剪贴板（`owned && !materialized`）时，**必须先执行同步物化再释放 `DataProvider` 引用**，防止当前剪贴板内容蒸发。

#### 核心接口与状态机实现

```rust
use parking_lot::RwLock;
use std::sync::Arc;
use crate::core::format_gate::PayloadTier;

#[derive(Debug, Clone)]
pub struct ActiveOwnershipState {
    pub item_id: String,
    pub content_hash: String,
    pub change_count_at_own: isize,
    pub copy_timestamp_ms: u64,
    pub source_bundle_id: String,
    pub raw_text: Arc<str>,
    pub raw_html: Option<Arc<str>>,
    pub matched_recipe_id: String,
    pub plain_text_policy: PlainTextPolicy,
    pub payload_tier: PayloadTier,
    pub materialized: bool,
}

pub trait ClipboardPlatformOwner: Send + Sync {
    /// 读取当前系统剪贴板序列号 (macOS changeCount / Win GetClipboardSequenceNumber)
    fn current_change_count(&self) -> isize;

    /// 零拷贝读取当前剪贴板元数据 (types 列表与 itemCount)，绝不拉取私有二进制数据
    fn inspect_metadata(&self) -> ClipboardMetadataSnapshot;

    /// 原子接管剪贴板：仅当当前 changeCount 仍等于 snapshot_change_count 时才执行 clearContents 并挂载 Provider
    fn acquire_ownership_cas(
        &self,
        snapshot_change_count: isize,
        state: ActiveOwnershipState,
    ) -> Result<isize, OwnershipAcquireError>;

    /// 暂停自动挡或主动放弃接管前：先物化当前持有内容，再释放 Provider 强引用
    fn materialize_and_release(&self, materializer: &crate::core::materializer::Materializer);

    /// 释放内部 Provider 强引用（当检测到外部新复制已夺走所有权时调用）
    fn release_provider_reference(&self);
}
```

#### macOS `objc2` `NSPasteboardItemDataProvider` 内存安全实现要点

1. **强引用生命周期绑定与按 `PayloadTier` 声明通道**：`ClipboardOwner` 内部维护 `active_provider: Mutex<Option<Retained<MacPasteboardProvider>>>`。每次调用 `acquire_ownership_cas` 时：
   * 先构造新的 `Retained<MacPasteboardProvider>`（内含 `Arc<ActiveOwnershipState>`、对 `WindowObserver` 的 `Arc` 引用、对 `PipeExecutor` 的 `Arc` 引用）；
   * 创建 `NSPasteboardItem`：
     * 若 `state.payload_tier == PayloadTier::FullMultiChannel`（`≤ 64KB`），调用 `item.setDataProvider_forTypes(&provider, &[NSPasteboardTypeHTML, NSPasteboardTypeRTF, NSPasteboardTypeString])`（严格遵守 §8.3 铁律 R1：**始终显式注册 `public.utf8-plain-text`**）；
     * 若 `state.payload_tier == PayloadTier::LightweightTextOnly`（`64KB ~ 256KB`），仅注册 `&[NSPasteboardTypeString]`，跳过自动挡富文本通道声明，保证大文本粘贴回调耗时 `< 1ms`；
   * **执行 TOCTOU 原子校验**：
     ```rust
     if pb.changeCount() != snapshot_change_count {
         return Err(OwnershipAcquireError::ConcurrentModificationDetected);
     }
     let new_count = pb.clearContents();
     let ok = pb.writeObjects(&NSArray::from_slice(&[item.as_ref()]));
     ```
   * 将 `new_count` 登记至 `PrivacyAndLoopGuard::register_internal_write(new_count, &state.content_hash)`，并将 `provider` 存入 `active_provider` 强引用槽。

### 3.2 模块 2：`FormatGate`（两阶段零开销元数据与体积分级门控）

#### 核心职责
将门控拆分为严格的两阶段：
* **第一阶段（`evaluate_metadata_gate`，0 字节正文读取）**：在读取任何剪贴板正文前，仅检查 `types` 字符串数组与 `itemCount`（Step 0A 实测耗时 `0.125 µs – 3.416 µs`）。将 G1 细分为 **`G1A_COMPLEX_OBJECT`（电子表格/OLE/设计软件私有对象无条件拦截）** 与 **`G1B_STANDALONE_MEDIA`（纯图片/截图无文本伴生时拦截）**——既彻底杜绝拉取 Excel / Figma / Adobe 的延迟渲染数据（Promised Data），又**精准放行携带伴生 `public.tiff` / `public.pdf` 预览图的 Word / Pages / Apple Mail 富文本复制**。
* **第二阶段（`evaluate_payload_tier`，双层体积分级）**：仅当 Stage 1 通过并读取 UTF-8 文本后，按字节长度划分为 `≤ 64KB`（`FullMultiChannel`）、`64KB ~ 256KB`（`LightweightTextOnly` 轻量纯文本降级，实测 `120KB` 单遍 UTF-8 净化耗时仅 `0.48ms`）与 `> 256KB`（`G5Oversize` 拒绝接管）。

#### 核心实现逻辑

```rust
pub const FULL_CHANNEL_MAX_BYTES: usize = 64 * 1024;        // 64KB: 全量三通道供给上限
pub const LIGHTWEIGHT_TEXT_MAX_BYTES: usize = 256 * 1024;   // 256KB: 轻量纯文本净化上限

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum GateSkipReason {
    #[serde(rename = "G1A_COMPLEX_OBJECT")]
    G1aComplexObject,   // Excel / OLE / iWork / Adobe / Figma / Sketch / RTFD 私有复杂对象
    #[serde(rename = "G1B_STANDALONE_MEDIA")]
    G1bStandaloneMedia, // 仅有 TIFF / PNG / PDF 且不含现代文本通道 (纯图片/截图)
    #[serde(rename = "G2_MULTI_ITEM")]
    G2MultiItem,        // pasteboardItems.count > 1
    #[serde(rename = "G3_FILE_URL")]
    G3FileUrl,          // 含 public.file-url 或 NSFilenamesPboardType
    #[serde(rename = "G4_LEGACY_ONLY")]
    G4LegacyOnly,       // 既无 public.utf8-plain-text 也无 public.html
    #[serde(rename = "G5_OVERSIZE")]
    G5Oversize,         // 文本字节长度 > 256KB
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PayloadTier {
    #[serde(rename = "FULL_MULTI_CHANNEL")]
    FullMultiChannel,    // <= 64KB: 注册 html + rtf + plainText
    #[serde(rename = "LIGHTWEIGHT_TEXT_ONLY")]
    LightweightTextOnly, // 64KB ~ 256KB: 仅注册 plainText 单遍轻量净化，支持手动触发富文本
}

/// 第一阶段：纯元数据检查（必须在读取任何剪贴板正文前调用，0 字节数据拉取）
pub fn evaluate_metadata_gate(
    declared_types: &[String],
    item_count: usize,
) -> Result<(), GateSkipReason> {
    if item_count > 1 {
        return Err(GateSkipReason::G2MultiItem);
    }
    let has_modern_text = declared_types
        .iter()
        .any(|t| t == "public.utf8-plain-text" || t == "public.html");
    let mut has_companion_media = false;

    for t in declared_types {
        if t == "public.file-url" || t == "NSFilenamesPboardType" {
            return Err(GateSkipReason::G3FileUrl);
        }
        // G1A: 电子表格、OLE 嵌入对象、设计软件图层或富文本附件包 -> 无条件拦截
        if t.starts_with("com.microsoft.excel.")
            || t == "com.microsoft.Object-Descriptor"
            || t == "com.microsoft.DataObject"
            || t.starts_with("com.apple.iWork.TS")
            || t.starts_with("com.adobe.")
            || t.starts_with("com.figma.")
            || t.starts_with("com.bohemiancoding.")
            || t == "com.apple.flat-rtfd"
        {
            return Err(GateSkipReason::G1aComplexObject);
        }
        // G1B: 图像与 PDF 类型（若伴生现代文本通道则为 Word/Pages 富文本预览图，予以放行）
        if t == "public.tiff" || t == "public.png" || t == "public.pdf" {
            has_companion_media = true;
        }
    }

    if has_companion_media && !has_modern_text {
        return Err(GateSkipReason::G1bStandaloneMedia);
    }
    if !has_modern_text {
        return Err(GateSkipReason::G4LegacyOnly);
    }
    Ok(())
}

/// 第二阶段：仅当 Stage 1 通过并读取 UTF-8 文本后，判定载荷体积分级
pub fn evaluate_payload_tier(text_byte_len: usize) -> Result<PayloadTier, GateSkipReason> {
    if text_byte_len <= FULL_CHANNEL_MAX_BYTES {
        Ok(PayloadTier::FullMultiChannel)
    } else if text_byte_len <= LIGHTWEIGHT_TEXT_MAX_BYTES {
        Ok(PayloadTier::LightweightTextOnly)
    } else {
        Err(GateSkipReason::G5Oversize)
    }
}
```

### 3.3 模块 3：`WindowObserver`（零分配 `Arc` 环形缓冲与 O(1) 前台感知）

#### 核心职责
1. 订阅系统级窗口激活事件（macOS `NSWorkspace.didActivateApplicationNotification`），以 `Arc<AppActivationRecord>` 写入容量为 16 的内存环形缓冲。
2. 为复制阶段提供抗竞态 `resolve_source_app(copy_ts_ms)`（解决复制后 `30ms` 极速 `⌘Tab` 切窗导致源应用识别错误的问题）。
3. 为粘贴回调提供 **零堆内存分配（仅原子递增引用计数 `Arc::clone`）** 的 `0.042 µs` 级 O(1) `current_frontmost_app()` 查询。

```rust
use parking_lot::RwLock;
use std::collections::VecDeque;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct AppActivationRecord {
    pub bundle_id: String,
    pub app_name: String,
    pub process_name: String,
    pub pid: i32,
    pub timestamp_ms: u64,
}

pub struct WindowObserver {
    ring: RwLock<VecDeque<Arc<AppActivationRecord>>>,
}

impl WindowObserver {
    pub const CAPACITY: usize = 16;
    pub const STALE_THRESHOLD_MS: u64 = 500;

    /// O(1) 零分配读取当前前台应用（粘贴回调内调用，仅克隆 Arc 指针，实测 p50 = 0.042 µs）
    #[inline]
    pub fn current_frontmost_app(&self) -> Option<Arc<AppActivationRecord>> {
        self.ring.read().back().cloned()
    }

    /// 抗竞态回溯复制瞬间的源应用
    pub fn resolve_source_app(
        &self,
        copy_ts_ms: u64,
    ) -> (Option<Arc<AppActivationRecord>>, ResolutionConfidence) {
        let guard = self.ring.read();
        for rec in guard.iter().rev() {
            if rec.timestamp_ms <= copy_ts_ms {
                let delta = copy_ts_ms.saturating_sub(rec.timestamp_ms);
                let conf = if delta <= Self::STALE_THRESHOLD_MS {
                    ResolutionConfidence::Exact
                } else {
                    ResolutionConfidence::Stale
                };
                return (Some(Arc::clone(rec)), conf);
            }
        }
        (guard.front().cloned(), ResolutionConfidence::Stale)
    }
}
```

### 3.4 模块 4：`PipeExecutor`（启发式通道分发、双通道防劫持抑制与 L1 算子路由）

#### `provideDataForType` 内的启发式路由决策树

当宿主应用请求通道 `requested_channel`（`Html` / `Rtf` / `PlainText`）时，`PipeExecutor::provide_channel_data` 按以下确定性顺序执行：

1. **后台嗅探保护窗（`< 150ms` Probe Guard）**：
   * 计算 `elapsed_ms = now_ms().saturating_sub(state.copy_timestamp_ms)`；
   * 若 `elapsed_ms < 150` 且 `frontmost_bundle_id == state.source_bundle_id`：说明是复制发生瞬间的后台守护进程（如 `sharingd`）或剪贴板管理器抢读，**不记录为有效用户粘贴**，对 `PlainText` 直接返回 `state.raw_text`，对 `Html` 返回 `state.raw_html`。
2. **`Html` 与 `Rtf` 双通道同步防劫持抑制分支**：
   * 检查 `frontmost_bundle_id` 是否在配方的 `suppressForTargetBundles` 集合中（默认包含：`com.tinyspeck.slackmacgap`、`com.hnc.Discord`、`com.microsoft.VSCode`、`com.todesktop.230313mzl4w4u92` [Cursor]、`com.apple.Terminal`、`com.googlecode.iterm2`）；
   * **为什么必须同步抑制 `Html` 与 `Rtf`**：Electron / 富文本应用在剪贴板协商时按 `public.html -> public.rtf -> public.utf8-plain-text` 顺序拉取；若只抑制 `Html` 而不抑制 `Rtf`，Slack / Discord 会回退读取 `Rtf` 通道，依然造成富文本污染；
   * 若命中抑制名单，对 `Html` 与 `Rtf` 均返回 `None`（分别记录 `applied_variant = "html_suppressed"` / `"rtf_suppressed"`），使目标应用干净回退读取 `PlainText` 通道（注：若 Step 0B 跨进程冒烟发现某特定目标应用在收到 `None` 时未自动回退读取 `PlainText`，则对该 Bundle ID 的 `Html` 通道返回 `<pre>` HTML 转义源码字符串作为无损兜底）；
   * 若未命中抑制名单：
     * `Html` 通道调用 `handlers::markdown_html::markdown_to_cf_html(&state.raw_text)` 返回带 `mso-` 专有样式的富文本 HTML；
     * `Rtf` 通道调用 `handlers::markdown_rtf::markdown_to_rtf(&state.raw_text)` 返回标准 RTF 字节流。
3. **`PlainText` 通道动态分支（按 `plainTextPolicy`）**：
   * `PlainTextPolicy::DynamicByTarget`：检查 `frontmost_bundle_id` 是否属于 `imTargetBundles`（`com.tencent.xinWeChat`、`com.tencent.WeWorkMac`、`com.alibaba.DingTalkMac`、`com.electron.lark`）；若是则实时调用单遍 UTF-8 字节流算子 `markdown_to_im_pretty_text(&state.raw_text)`（实测 4KB 耗时 `17.12 µs`，120KB 长文耗时 `0.48 ms`）返回去星号精致纯文本，否则返回 `state.raw_text`（原始无损 Markdown）。
   * `PlainTextPolicy::Cleaned`：调用对应清洗算子（如 `social_link_cleaner::clean_social_links(&state.raw_text)`）。
   * `PlainTextPolicy::Passthrough`：直接返回 `state.raw_text`。
   * `PlainTextPolicy::SanitizedOptIn`：调用 `dlp_detector::redact_secrets(&state.raw_text)` 返回 `<REDACTED>` 脱敏文本。

### 3.5 模块 5：`CallbackBudget`（FFI 异常隔离与 8ms 预算监控）

> **工程语义说明**：由于 macOS `provideDataForType` 运行在 AppKit 主线程同步 FFI 回调内，Rust 无法在不引发 Objective-C 栈帧未定义行为（UB）的前提下强行 `pthread_cancel` 抢占正在执行的同步指令。因此，「`< 8ms` 不超时」通过以下双重防线保障：
> 1. **事前输入边界锁定（预防层）**：Stage 2 `FormatGate` 限制全量转换 `≤ 64KB`（实测 max=`4.96ms`），`64KB~256KB` 仅执行单遍 UTF-8 纯文本扫描（实测 `120KB` max=`0.67ms`），配合 `regex` 线性时间 DFA 自动机，从算法复杂度上保证不超 `8ms`；
> 2. **事中 Panic 隔离与事后超时审计降级（兜底层）**：`execute_with_budget` 使用 `catch_unwind` 100% 拦截 FFI 边界 Panic，并在算子耗时超出 `8ms` 时记录 `degraded_timeout = 1` 并安全回退至 `raw_fallback`。

```rust
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Instant;

pub const CALLBACK_HARD_TIMEOUT_MS: u128 = 8;

pub fn execute_with_budget<F>(
    raw_fallback: &str,
    f: F,
) -> (Option<String>, bool /* degraded */)
where
    F: FnOnce() -> Option<String>,
{
    let t0 = Instant::now();
    let result = catch_unwind(AssertUnwindSafe(f));
    let elapsed_ms = t0.elapsed().as_millis();

    match result {
        Ok(Some(output)) if elapsed_ms <= CALLBACK_HARD_TIMEOUT_MS => (Some(output), false),
        Ok(None) => (None, false), // 显式抑制（如 Slack HTML/RTF 双通道抑制）
        Ok(Some(_)) => {
            tracing::warn!(elapsed_ms, "Provider callback exceeded 8ms budget; falling back to raw text");
            (Some(raw_fallback.to_owned()), true)
        }
        Err(panic_payload) => {
            tracing::error!(?panic_payload, "Caught panic across FFI boundary in provider callback");
            (Some(raw_fallback.to_owned()), true)
        }
    }
}
```

### 3.6 模块 6：`Materializer`（主线程安全的生命周期与暂停态同步物化）

#### 核心职责
解决延迟供给架构下进程退出或暂停自动挡后系统剪贴板变空（`types=[]`）的缺陷（附录 A.8），同时避免日常运行期 1s 定时物化带来的粘贴竞态。

#### 触发挂载点与异步信号安全规范（严禁裸 POSIX Signal Handler 调 AppKit）
> **⚠️ Async-Signal-Safety 铁律**：在 Unix 裸 `signal()` / `sigaction()` 异步信号处理函数内，绝对禁止分配堆内存、获取非重入锁或调用 Objective-C `NSPasteboard`（若信号到达时主线程正持有 malloc 或 AppKit 锁，会立即死锁）。

1. **macOS 正常退出**：Tauri `RunEvent::ExitRequested` / `NSApplicationDelegate::applicationWillTerminate`（已在主线程）。
2. **Unix 信号安全退出（macOS GCD Signal Source / Tokio Signal）**：
   * 在启动时先调用 `libc::signal(SIGTERM, libc::SIG_IGN)` 与 `libc::signal(SIGINT, libc::SIG_IGN)` 屏蔽默认裸中断；
   * 通过 macOS GCD `dispatch_source_create(DISPATCH_SOURCE_TYPE_SIGNAL, SIGTERM/SIGINT, 0, dispatch_get_main_queue())`（或 `tokio::signal::unix::signal` 接收后通过 `app_handle.run_on_main_thread` 派发回主线程），确保物化逻辑 **100% 在 AppKit 主线程事件循环中安全执行**，执行完毕后再调用 `std::process::exit(0)`。
3. **暂停自动挡前置保护**：当 `cmd_set_autopilot_enabled(false)` 被调用时，在主线程先调用 `Materializer::materialize_current()`，再释放 `DataProvider`。
4. **Rust Panic 崩溃退出**：在 `main.rs` 初始化时通过 `std::panic::set_hook` 串联原有 Hook，若当前线程为主线程则尝试执行一次同步物化。

#### 临终物化执行协议（四步铁律）
1. 读取当前 `ActiveOwnershipState`；若 `owned == false` 或 `materialized == true`，直接返回。
2. **所有权前置校验**：读取系统当前 `pb.changeCount()`，断言 `pb.changeCount() == state.change_count_at_own`；若不等，说明剪贴板已被其他应用的新复制接替，**立即中止物化**。
3. 同步计算 `html`、`rtf`（若为 `FullMultiChannel`）与 `plain_text`（按保守默认策略），调用 `pb.clearContents()` 并静态写入通道的真实字节数据。
4. 将新生成的 `changeCount` 登记至 `PrivacyAndLoopGuard` 并标记 `state.materialized = true`。

### 3.7 模块 7：`DoubleCopyDetector`（零权限双击检测与 260ms 消抖协同）

```rust
pub const DOUBLE_COPY_WINDOW_MS: u64 = 250;
pub const TOAST_DEBOUNCE_DELAY_MS: u64 = 260;

pub struct DoubleCopyDetector {
    last_external_copy: Mutex<Option<(u64 /* ts_ms */, String /* hash */, String /* source_bundle */)>>,
    pending_toast_abort: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}
```

* **单次复制流程**：接管成功后，启动 `260ms` 异步定时器等待通过 `NativeToastController` 弹出预告 Toast，并将取消句柄存入 `pending_toast_abort`。
* **双击命中流程**（`Δt < 250ms && hash_eq && source_eq`）：
  1. 立即通过 `pending_toast_abort` 取消尚未弹出的自动挡预告 Toast；
  2. **v0.1.0 极简版行为**：将剪贴板还原为纯文本原始态（不挂富文本转换），并通过 `NativeToastController` 弹出 `↩️ 已还原为原始纯文本` 微提示；
  3. **v0.1.5 完整交互版行为**：对第二次复制继续完成自动挡重接管（Re-own），同时在鼠标/光标处唤醒常驻隐藏的 `Micro-HUD`。

### 3.8 模块 8：`PrivacyAndLoopGuard`（防死循环、黑名单与静默三不 DLP）

1. **不可关闭的密码管理器与隐私标记黑名单**：
   * 进程 Bundle ID / 名称黑名单：`com.1password.1password`、`com.bitwarden.desktop`、`org.keepassxc.keepassxc`、`com.apple.keychainaccess`、`in.sinew.Enpass-Desktop`。
   * 剪贴板隐私类型黑名单：`org.nspasteboard.ConcealedType`、`org.nspasteboard.TransientType`、`ExcludeClipboardContentFromMonitorProcessing`。
2. **高置信度密钥静默识别（预编译 `LazyLock<Regex>`，不含手机号与身份证号）**：
   ```rust
   use std::sync::LazyLock;
   use regex::Regex;

   pub static SECRET_REGEX: LazyLock<Regex> = LazyLock::new(|| {
       Regex::new(
           r"(?:sk-[A-Za-z0-9_\-]{20,}|ghp_[A-Za-z0-9]{30,}|AKIA[0-9A-Z]{16}|-----BEGIN [A-Z ]+PRIVATE KEY-----|postgres(?:ql)?://[^:\s]+:[^@\s]+@|mongodb(?:\+srv)?://[^:\s]+:[^@\s]+@)"
       ).expect("Valid static DLP regex")
   });
   ```
   * 命中 `SECRET_REGEX` 时：返回 `DlpVerdict::SilentBypass`——**不接管剪贴板（保持原生明文粘贴）、不写入 SQLite 历史库、禁止送入 L3 云端接口**。

### 3.9 全局快捷键唯一常量表 (`src/core/hotkeys.rs`)

与《Master Spec v1.4 §4.5》及《UI Spec v1.0》严格对齐：

```rust
pub struct HotkeySpec {
    pub id: &'static str,
    pub macos_accelerator: &'static str,
    pub windows_accelerator: &'static str,
    pub description: &'static str,
}

pub const GLOBAL_HOTKEYS: &[HotkeySpec] = &[
    HotkeySpec {
        id: "toggle_autopilot",
        macos_accelerator: "Command+Shift+P",
        windows_accelerator: "Ctrl+Shift+P",
        description: "全局暂停 / 恢复自动挡",
    },
    HotkeySpec {
        id: "summon_micro_hud",
        macos_accelerator: "Control+Alt+V",
        windows_accelerator: "Ctrl+Alt+V",
        description: "手动挡备选热键呼出 Micro-HUD (v0.1.0 还原原文)",
    },
    HotkeySpec {
        id: "explicit_dlp_sanitize",
        macos_accelerator: "Control+Alt+D",
        windows_accelerator: "Ctrl+Alt+D",
        description: "显式将当前剪贴板敏感密钥替换为 <REDACTED>",
    },
    HotkeySpec {
        id: "toggle_drawer",
        macos_accelerator: "Alt+Shift+V",
        windows_accelerator: "Alt+Shift+V",
        description: "呼出 / 隐藏磁吸式历史与贪吃蛇抽屉 (v0.1.5)",
    },
    HotkeySpec {
        id: "toggle_stack_mode",
        macos_accelerator: "Alt+Shift+S",
        windows_accelerator: "Alt+Shift+S",
        description: "开启 / 关闭贪吃蛇连续收集槽模式 (v0.1.5)",
    },
];

pub const FORBIDDEN_HOTKEYS: &[&str] = &["Alt+V", "Option+V", "Command+V+V", "Ctrl+V+V"];
```

## 4. L1 核心确定性算子实现规范 (L1 Handlers Specification)

### 4.1 【AI 星号净化器】Word / 飞书保真 `mso-` HTML 与单遍 UTF-8 纯文本生成 (`markdown_html.rs` / `markdown_im_text.rs`)

1. **Word / 飞书 `mso-` HTML 生成规范 (`markdown_html.rs`)**：
   Word 桌面端使用自有 HTML 渲染引擎，忽略大部分现代 CSS。生成的 HTML 必须内联以下 `mso-` 专有样式以确保粘贴到 Word 与飞书文档时均具有原生表格边框与标题大纲层级：
   * **标题大纲映射**：`<h1>`–`<h6>` 必须附带 `style="mso-outline-level:1; font-family:'PingFang SC','Microsoft YaHei',sans-serif;"`。
   * **实线边框表格**：`<table>` 必须声明 `border="1" cellspacing="0" cellpadding="6" style="border-collapse:collapse; border:1px solid #BFBFBF; mso-border-alt:solid #BFBFBF .5pt; mso-padding-alt:4pt 6pt 4pt 6pt;"`；每个 `<th>` 与 `<td>` 必须显式携带 `style="border:1px solid #BFBFBF; mso-border-alt:solid #BFBFBF .5pt;"`。
   * **代码块保真**：`<pre><code>` 转换为单格浅灰底色框或 `<p style="background:#F6F8FA; font-family:Menlo,Consolas,monospace; white-space:pre-wrap;">`，防止粘贴进 Word 后换行丢失。
2. **微信 / 钉钉 IM 单遍 UTF-8 字节流净化算子 (`markdown_im_text.rs`)**：
   * 直接在 `&[u8]` 切片上执行单遍状态机扫描（无需逐行拆解 Unicode Grapheme 或反复调用正则桥接），将行首 `# Title` 转为 `【Title】`、`## Subtitle` 转为 `▍Subtitle`，剥离行内 `**` 与 `` ` `` 围栏。
   * **性能实测**：4KB 常规回复耗时 **`17.12 µs`**；`120KB` 超长回复（`LIGHTWEIGHT_TEXT_ONLY` 模式）p50 = **`0.48 ms`**、max = **`0.67 ms`**。

### 4.2 【社媒/电商链接瘦身】预编译正则与域名分级口令清洗规范 (`social_link_cleaner.rs`)

* **严禁调用 `NSDataDetector`**（Step 0 实测其冷启动耗时 `8.77ms`，直接击穿回调预算）。
* 使用预编译 `LazyLock<Regex>` 提取文本中的 HTTP/HTTPS URL（实测耗时 `129 µs`），配合 `url::Url` 按**「全局通用追踪参数」与「特定社媒/电商域名专属参数」分级剥离**（防止误伤开发者后台链接中的业务查询参数 `?from=2026-01-01`）：
  * **全局无条件剥离参数（任意域名）**：`utm_source`, `utm_medium`, `utm_campaign`, `utm_term`, `utm_content`, `spm`, `vd_source`, `xhsshare`, `bc_fl_src`
  * **社媒/电商域名专属剥离参数（仅当 Host 匹配 `weixin.qq.com`、`xiaohongshu.com`、`xhslink.com`、`bilibili.com`、`b23.tv`、`tb.cn`、`taobao.com`、`tmall.com`、`jd.com` 时剥离）**：`from`, `isappinstalled`, `share_source`, `share_medium`, `share_plat`, `share_session_id`, `share_tag`, `unique_k`, `app_platform`, `app_version`, `share_from_user_hidden`, `apptime`, `gx`, `ad_od`
* 若分享文本包含“【标题】+ 链接 + 冗长口令引导词（如‘复制本条消息打开小红书/淘宝’）”，保留 `【标题】 净化后链接`，自动剥离尾部引导口令。

## 5. 本地存储 SQLite3 + FTS5 (`trigram`) 数据库设计 (Storage Schema)

数据库文件存放于应用数据目录 `pasteflow.db`，启动时开启 `PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;`。

> **为什么使用 `tokenize='trigram'` 并挂载同步触发器**：
> 1. SQLite 默认 `unicode61` 分词器按空白符切词，对无空格的中文连续句子（如“架构周报总结”）无法命中子串查询（用户输入“周报”会返回 0 条结果）。采用 SQLite 3.34+ 内置的 **`trigram` 分词器**可原生支持中英文任意 `≥ 3` 字符子串的毫秒级索引检索；当用户输入 `1~2` 个字符的短查询时，查询层自动回退为带 `LIMIT 50` 的 `LIKE '%q%'` 扫描。
> 2. 由于 `clipboard_fts` 采用外部内容表（`content='clipboard_items'`），必须显式创建 `AFTER INSERT / DELETE / UPDATE` 三个同步触发器，否则向主表插入数据后 FTS5 索引将为空。

```sql
-- 1. 剪贴板历史主表 (v0.1.5 启用历史抽屉时写入；DLP 命中项绝不落表)
CREATE TABLE IF NOT EXISTS clipboard_items (
    id TEXT PRIMARY KEY,                       -- UUID v7
    created_at INTEGER NOT NULL,               -- Unix Timestamp (ms)
    content_hash TEXT NOT NULL,                -- BLAKE3 Hash
    primary_type TEXT NOT NULL,                -- 'text' | 'html' | 'rtf' | 'image' | 'files'
    raw_plain_text TEXT,                       -- 原始纯文本
    raw_html TEXT,                             -- 原始 HTML (可选)
    image_rel_path TEXT,                       -- 图片缩略图相对路径
    source_app_name TEXT NOT NULL,
    source_bundle_id TEXT,
    source_confidence TEXT NOT NULL,           -- 'exact' | 'stale'
    tags_json TEXT NOT NULL,                   -- JSON array of ContentTags
    plain_text_policy TEXT NOT NULL,           -- 'DYNAMIC_BY_TARGET' | 'PASSTHROUGH' | 'CLEANED' | 'SANITIZED_OPT_IN'
    payload_tier TEXT NOT NULL DEFAULT 'FULL_MULTI_CHANNEL', -- 'FULL_MULTI_CHANNEL' | 'LIGHTWEIGHT_TEXT_ONLY'
    is_pinned INTEGER NOT NULL DEFAULT 0,
    in_stack INTEGER NOT NULL DEFAULT 0,
    stack_order INTEGER
);

CREATE INDEX IF NOT EXISTS idx_clipboard_created_at ON clipboard_items(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_clipboard_hash ON clipboard_items(content_hash);

-- 2. FTS5 毫秒级全文检索虚拟表 (使用 trigram 分词器支持中英文任意子串检索)
CREATE VIRTUAL TABLE IF NOT EXISTS clipboard_fts USING fts5(
    raw_plain_text,
    source_app_name,
    content='clipboard_items',
    content_rowid='rowid',
    tokenize='trigram'
);

-- 2.1 外部内容表 FTS5 自动同步触发器 (INSERT / DELETE / UPDATE)
CREATE TRIGGER IF NOT EXISTS clipboard_fts_ai AFTER INSERT ON clipboard_items BEGIN
    INSERT INTO clipboard_fts(rowid, raw_plain_text, source_app_name)
    VALUES (new.rowid, COALESCE(new.raw_plain_text, ''), new.source_app_name);
END;

CREATE TRIGGER IF NOT EXISTS clipboard_fts_ad AFTER DELETE ON clipboard_items BEGIN
    INSERT INTO clipboard_fts(clipboard_fts, rowid, raw_plain_text, source_app_name)
    VALUES ('delete', old.rowid, COALESCE(old.raw_plain_text, ''), old.source_app_name);
END;

CREATE TRIGGER IF NOT EXISTS clipboard_fts_au AFTER UPDATE OF raw_plain_text, source_app_name ON clipboard_items BEGIN
    INSERT INTO clipboard_fts(clipboard_fts, rowid, raw_plain_text, source_app_name)
    VALUES ('delete', old.rowid, COALESCE(old.raw_plain_text, ''), old.source_app_name);
    INSERT INTO clipboard_fts(rowid, raw_plain_text, source_app_name)
    VALUES (new.rowid, COALESCE(new.raw_plain_text, ''), new.source_app_name);
END;

-- 3. 粘贴通道实际供给审计与效率统计表 (供 §11.4 本地诊断面板与种子周报使用)
CREATE TABLE IF NOT EXISTS served_history_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    item_id TEXT NOT NULL,
    served_at INTEGER NOT NULL,
    channel TEXT NOT NULL,                     -- 'html' | 'rtf' | 'plainText'
    frontmost_bundle_id TEXT NOT NULL,
    applied_variant TEXT NOT NULL,             -- 'rich_office' | 'im_cleaned_text' | 'raw_passthrough' | 'html_suppressed' | 'rtf_suppressed'
    duration_us INTEGER NOT NULL,
    degraded_timeout INTEGER NOT NULL DEFAULT 0
);

-- 4. FormatGate 护栏拦截统计表 (不含任何剪贴板内容，仅统计拦截原因与频次)
CREATE TABLE IF NOT EXISTS gate_skip_metrics (
    day_bucket TEXT NOT NULL,                  -- 'YYYY-MM-DD'
    skip_reason TEXT NOT NULL,                 -- 'G1A_COMPLEX_OBJECT' | 'G1B_STANDALONE_MEDIA' | 'G2_MULTI_ITEM' | 'G3_FILE_URL' | 'G4_LEGACY_ONLY' | 'G5_OVERSIZE'
    hit_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (day_bucket, skip_reason)
);
```

## 6. 自动化测试与发版门禁规范 (Testing & CI Guardrails)

1. **L1 算子 AST、`mso-` 样式与域名分级瘦身快照测试 (`cargo test --test l1_snapshots`)**：
   * 固定输入 10 组真实 AI 回复语料（含跨行粗体、GFM 表格、代码块、B站/淘宝/小红书分享串、含 `?from=2026-01-01` 的开发者 API 链接），断言生成的 HTML 包含 `mso-border-alt`、生成的微信纯文本不含 `**`、社媒链接剥离追踪参数而开发者 API 链接的 `from` 参数 100% 保留。
2. **两阶段 `FormatGate`、双通道抑制与 `CallbackBudget` 边界回归测试 (`cargo test --test guardrails`)**：
   * 断言 Excel (`com.microsoft.excel.data`)、Figma (`com.figma.node`) 触发 `G1aComplexObject`；纯截图 (`["public.png", "public.tiff"]`) 触发 `G1bStandaloneMedia`；而 **Word/Pages 富文本复制 (`["public.utf8-plain-text", "public.html", "public.rtf", "public.tiff"]`) 100% 放行通过**。
   * 断言 `120KB` 长文进入 `PayloadTier::LightweightTextOnly` 且单遍 UTF-8 净化耗时 `< 1ms`；`600KB` 文本触发 `G5Oversize`。
   * 断言当 `frontmost_bundle_id == "com.tinyspeck.slackmacgap"` 时，`Html` 与 `Rtf` 两个通道均同步返回 `None`（`html_suppressed` & `rtf_suppressed`）。
   * 构造故意 Panic 的测试算子与超时算子，断言 `execute_with_budget` 100% 捕获 Panic 并在超时后安全返回 `raw_fallback` 且标记 `degraded = true`。
3. **快捷键无冲突断言 (`cargo test --test hotkey_registry`)**：
   * 遍历 `src/core/hotkeys.rs` 中定义的全部 5 个激活快捷键（`⌘⇧P`、`⌃⌥V`、`⌃⌥D`、`⌥⇧V`、`⌥⇧S`），断言 macOS 与 Windows 各自平台下任意两个功能的键位组合两两不相等，且无任何键位落入 `FORBIDDEN_HOTKEYS` 黑名单（`⌥V` / `⌘V⌘V`）。
