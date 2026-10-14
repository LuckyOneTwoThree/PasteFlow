# PasteFlow (灵贴) — 产品定义与架构源文档 (Master Spec v1.3)

> **文档状态：** 已归档（Superseded by [[PasteFlow (灵贴) — 产品定义与架构源文档 (Master Spec v1.4)]]）
> **修订日期：** 2026-10-14
> **文档性质：** 终态产品定义与系统架构源文档。本文档已剔除全部历史过程性批注与废案争辩，所有章节均以当前唯一生效的工程规范直述，可直接作为下游 UI Spec、Rust TDD、IPC Spec 与代码实现的唯一真源。
> **历史归档：** [[PasteFlow (灵贴) — 产品定义与架构源文档 (Master Spec v1.0)]] · [[PasteFlow (灵贴) — 产品定义与架构源文档 (Master Spec v1.1)]] · [[PasteFlow (灵贴) — 产品定义与架构源文档 (Master Spec v1.2)]]

## 0. 版本演进脉络摘要 (Version Evolution Summary)

| 版本 | 核心演进内容 | 关键工程收益 |
|---|---|---|
| **v1.0** | 确立产品定位、L1/L2/L3 三级引擎漏斗、Micro-HUD 微胶囊、贪吃蛇收集槽（Stack Mode）与声明式 `PipeRecipe` 协议 | 建立完整的产品形态与功能边界 |
| **v1.1** | 将自动挡架构由「拦截 `Ctrl/Cmd+V` 同步重写」重构为**「复制时持有 + 按需延迟供给（Owned Lazy Provision）」**；淘汰 `arboard` / `rdev`；确立 L2 OCR 框线优先策略与 macOS 优先 MVP 路线 | 消除全局键盘钩子超时静默卸载、杀软误报、粘贴读取竞态与原始内容破坏风险 |
| **v1.2** | 基于本机实测（附录 A.7–A.11）新增 **Tier 0/1 权限分层**、**`plainTextPolicy` 通道策略**、**接管前置护栏（G1–G4）**、**provider 8ms 硬超时**、**DLP 强制覆盖原文回贴**、**L3 原子粘贴**与 **快捷键全局登记表** | 堵住 AppKit 从 RTF 自动合成纯文本泄漏、legacy 格式静默丢失、主线程回调挂死宿主及敏感明文回贴漏洞 |
| **v1.3** | 解决多机制耦合的二阶冲突：**取消 1s 定时物化改为生命周期临终物化 + 首次拉取后惰性缓存**（消除 1s 粘贴竞态与 `changeCount` 三次跳变）、**`⌘C⌘C` 双击与 260ms 延迟 Toast 消抖 + 二次复制重接管（Re-own）**、**新增护栏 G5（源载荷体积上限）**、**统一 D1 HUD 常驻策略**，并将全文净化为无过程批注的终态规范 | 彻底消除新机制间的时序死锁与文档内部前后矛盾，达到可直接交付研发的工业级纯净度 |

## 1. 项目元信息与核心定位 (Project Metadata)

### 1.1 基本信息

| 属性 | 定义内容 |
|---|---|
| **项目名称** | **PasteFlow**（中文代号：**灵贴**） |
| **一句话 Pitch** | _Don't just copy and paste. Transform in-flight._（懂你正在用什么软件的“自动挡”智能剪贴板） |
| **产品本质** | 跨平台操作系统级数据清洗与流转中间件（OS-Level Context-Aware Clipboard Middleware） |
| **目标受众** | **大众层（80%）：** 日常重度使用办公软件、AI 助手、浏览器的白领、运营、学生、财务人员。<br>**极客层（20%）：** 全栈开发者、AI 工程师、效率工具控、开源社区贡献者。 |
| **首发平台** | **macOS 优先**（v0.1.0 仅交付 macOS）；Windows 于 v0.2.0 跟进；Linux 于 v0.3.0+ 演进。 |
| **核心差异化** | **持有式延迟供给（Owned Lazy Provision）**：PasteFlow 不改写用户复制的内容本身，而是在复制瞬间持有剪贴板并声明多格式通道，由消费方按需拉取对应通道——用户按的仍是原生 `Cmd/Ctrl+V`，Word 自动拿到原生排版富文本，Terminal 自动拿到原始 Markdown，**主链路（Tier 0）零键盘钩子、零系统权限申请，且源格式经护栏受控保全**。 |

### 1.2 差异化边界声明

对外宣传、README、社区投稿与演示物料必须严格遵守以下可声称边界：

1. **首个在原生 `Cmd/Ctrl+V` 上以「持有式延迟供给」实现主链路零权限、零钩子、多格式按需分发的开源剪贴板中间件。** 现有工具要么在复制时急切改写（CopyQ、ClipboardFusion），要么依赖备用快捷键（PowerToys Advanced Paste、PasteMD）。对外表述须明确限定“主链路（Tier 0）零权限”与“经受控保全后原文可随时取回”，不作无条件的“全功能零权限”或“绝对无损”承诺。
2. **首个跨 macOS + Windows 的开源转换型剪贴板。** PasteMD 与 PowerToys Advanced Paste 仅支持 Windows；Maccy 仅支持 macOS 且仅做历史管理。
3. **首个支持「一句话自然语言生成声明式滤镜配方」（AI Pipe Builder）的剪贴板工具。**
4. **首个在连续收集槽（Stack Mode）消费阶段内置 AI 结构化聚合整理的剪贴板工具。**

### 1.3 指标体系

**一、产品北极星指标（可控核心指标）**

* **日均有效转换触发次数**：活跃用户日均触发自动挡或手动挡转换 ≥ **20 次**。
* **核心留存率**：安装后 **D7 留存率 ≥ 25%**。
* **社区生态飞轮**：社区提交并合入官方仓库的 `PipeRecipe` 配方数量。

**二、工程性能与资源指标**

| 指标项 | 目标值 | 实测基线与工程约束 |
|---|---|---|
| **安装包体积** | `< 10MB` | Tauri 2.0 Release 实测 exe `9.02MB` / MSI `3.00MB`（不内置本地 OCR 权重文件）。 |
| **Rust 核心进程常驻内存** | `< 25MB` | 指空闲时 Rust 核心进程 RSS（不含 Webview 进程组）。 |
| **整应用空闲内存（含常驻 HUD）** | 待 Step 0 实测锁定 | 采用常驻隐藏 `micro-hud` `WebviewWindow`（D1 决议），由 Step 0 Spike 在 macOS 实测 WKWebView 空闲内存后填入基准值。 |
| **L1 本地转换延迟** | `< 5ms`（硬超时 `8ms`） | 仅在消费方拉取通道时执行；provider 回调设 `8ms` 硬超时，超时立即回退原始内容。 |
| **L2 原生 OCR 延迟** | 热态 `< 100ms` | macOS Vision `.accurate` 热态实测 `52–70ms`；机器级首次调用实测 `13,554ms`，必须启动后台预热并配置 Loading 态，且**严禁进入 provider 回调**。 |
| **Tier 0 权限申请数** | **0 项** | 覆盖自动挡主链路、`⌘C⌘C` 差分双击检测、HUD 鼠标定位与剪贴板写回。 |
| **Tier 1 权限申请数** | **1 项（Accessibility）** | 覆盖 AXUIElement 精确输入光标定位、`enigo` 自动按键注入与 L3 打字机流式输出，须由用户显式开启。 |

**三、开源传播里程碑（非可控结果指标）**

* GitHub Stars：v0.1.0 发布后 3 个月达成 `1k`；12 个月达成 `5k`；24 个月以上冲刺 `10k+`。

## 2. 核心设计原则 (Design Principles)

任何子文档与代码实现必须严格遵循以下 **7 条核心铁律**（当原则间发生冲突时，优先级顺序为：**原则 4 > 原则 7 > 原则 2 > 原则 1 > 原则 3 > 原则 5 > 原则 6**）：

1. **主链路零权限开箱即用 (Zero-Permission Tier 0 Out-of-the-Box)**
   核心高频功能基于本地确定性算法（L1）与系统原生视觉能力（L2）构建。用户下载安装后第 1 秒**无需联网、无需注册、无需配置 API Key、无需授予任何系统权限**，即可完整体验自动挡转换与 `⌘C⌘C` 微胶囊交互。
2. **肌肉记忆零妥协 (Zero Muscle-Memory Friction)**
   绝不破坏用户养成的原生 `Ctrl/Cmd + C` 与 `Ctrl/Cmd + V` 肌肉记忆。**任何会给日常粘贴增加可感知阻塞的设计（如拦截 `Ctrl+V` 等待双击、在粘贴回调内同步执行 OCR 或网络请求）一律否决。**
3. **无焦点抢占 (Non-Activating UI)**
   任何悬浮提示（Toast）与光标微胶囊（Micro-HUD）弹出时，**绝对不能抢夺当前活动窗口的输入焦点**。macOS 与 Windows 的无焦点键盘路由机制不对称，必须按平台分别实现。
4. **本地隐私绝对优先 (100% Local-First & Privacy Guard)**
   所有剪贴板历史仅存储于本地 SQLite；内置不可关闭的密码管理器黑名单与系统隐私标记过滤；当内容命中敏感凭证（`contains_secret`）时，安全脱敏策略强制高于原文保留原则，严禁明文回贴或外泄。
5. **原始内容受控保全 (Controlled Non-Destructive Preservation)**
   PasteFlow 从不主动篡改用户复制的原始数据。除命中 `contains_secret` 的安全锁定期外，原始文本与可直通的源格式均完整保存在进程与本地库中，支持通过 HUD 或历史抽屉随时取回。
6. **声明式可扩展 (Declarative Extensibility)**
   所有转换能力抽象为机器可严格校验的标准化 `PipeRecipe` JSON 协议（开启 `additionalProperties: false`），支持单文件导入导出与自然语言生成，便于社区贡献。
7. **宁可不接管，不可破坏原生粘贴 (Never Degrade Native Paste)**
   任何一次“接管剪贴板”的决定，都必须以“用户的 `Cmd/Ctrl+V` 仍能粘出至少与不接管时等价的内容”为前提。遇到多文件复制、纯 Legacy 格式源、超大载荷或格式保全不完整时，**必须主动放弃接管**，退回系统原生剪贴板行为。

## 3. 核心架构决策：持有式延迟供给与权限分层 (Core Architecture)

### 3.1 为什么摒弃“粘贴时按键拦截重写”架构

以全局键盘钩子拦截 `Ctrl/Cmd+V` 并在 `<1ms` 内同步重写剪贴板的传统方案，在现代操作系统上存在 6 个不可调和的系统级缺陷：

1. **钩子超时静默卸载**：Windows `WH_KEYBOARD_LL` 回调一旦超过 `LowLevelHooksTimeout`，Win7+ 系统不仅放行按键，还会**静默卸载钩子且不通知应用**，导致功能无征兆失效。
2. **违背系统线程准则**：微软官方规范要求低阶钩子立即返回，严禁在钩子内执行同步剪贴板打开、加锁与多格式重写。
3. **杀软高误报率**：包含全局低阶键盘钩子的未签名或新签名二进制极易被 Windows Defender 与主流杀软标记为木马（如 `Trojan:Win32/Fathale`）。
4. **异步读取竞态崩溃**：剪贴板是全局共享资源，在目标应用异步读取过程中原地改写剪贴板，会触发目标应用模态报错（如 `operation did not succeed`）。
5. **破坏性覆写**：原地重写会物理销毁剪贴板中的原始文本与源应用私有格式，用户贴完 Word 后无法再贴回 IDE 获取原始 Markdown。
6. **高危权限墙**：macOS `CGEventTap` 强依赖 Input Monitoring 与 Accessibility 权限，首次启动即弹出高危系统警告，且在 Sequoia/Tahoe 下撤销权限时活跃的 Event Tap 存在引发系统级输入卡死的风险。

### 3.2 持有式延迟供给机制 (Owned Lazy Provision)

**核心洞察：绝大多数跨软件粘贴场景无需识别目标应用是谁，操作系统的原生格式协商（MIME / UTI Negotiation）天然具备目标分发能力。**

1. **复制接管（Capture & Own）**：当用户按下原生 `Cmd/Ctrl+C` 时，若源内容通过前置护栏（§8.9）并命中激活滤镜，PasteFlow 接管剪贴板所有权，声明 `public.html`、`public.rtf`、`public.utf8-plain-text` 及源剪贴板原有的合法直通 UTI 通道，此时**不执行任何格式转换计算**。
2. **按需供给（Lazy Provision）**：当用户在目标应用中按下原生 `Cmd/Ctrl+V` 时，目标应用根据自身属性向系统剪贴板请求偏好通道，触发 PasteFlow 的 DataProvider 回调，**在 `<5ms` 内现场计算并返回对应通道产物**：
   * **富文本消费方（Word / 飞书 / 钉钉文档 / WPS / Pages）**：优先请求 `public.html` 或 `public.rtf`，获得带原生标题层级、加粗与边框表格的富文本。
   * **纯文本消费方（Terminal / VSCode / 微信桌面端 / 记事本）**：请求 `public.utf8-plain-text`，根据当前滤镜的 `plainTextPolicy`（§5.4）获得原始无损文本（`PASSTHROUGH`）或清洗/脱敏后的纯文本（`CLEANED` / `SANITIZED_FORCED`）。
   * **原生私有格式消费方（Excel 内部粘贴 / Chrome 富组件粘贴）**：请求各自的私有 UTI（如 `com.microsoft.excel.data`），由 `FormatPreserver` 原样返回接管时缓存的源字节流。

### 3.3 平台底层实现机制

| 平台 | 核心 API 与机制 | 工程实现要点 |
|---|---|---|
| **macOS** | `NSPasteboardItem` + `NSPasteboardItemDataProvider` | 通过 `objc2` 调用 `setDataProvider(_:forTypes:)` 声明通道；实现 `pasteboard(_:item:provideDataForType:)` 按请求供给。Provider 实例必须在 Rust 侧强引用保活，直至所有权丢失或进程退出。 |
| **Windows** | `OleSetClipboard` + 自实现 `IDataObject` | 通过 `windows-rs` 自实现 COM `IDataObject` 接口（延迟渲染，为目标 `FORMATETC` 提供按需 `GetData` 分发），退出前通过 `OleFlushClipboard` 完成物化。 |

### 3.4 架构特性与边界约束汇总

| 维度 | 架构表现与硬性边界 | 对应机制章节 |
|---|---|---|
| **按键拦截与钩子** | 自动挡主链路与 `⌘C⌘C` 双击检测**完全不安装全局键盘钩子**，零按键延迟。 | §3.5 / §4.1 |
| **单通道策略冲突** | `public.utf8-plain-text` 同一时刻仅有一份返回值。通过 per-filter `plainTextPolicy` 枚举解决，Markdown 与代码滤镜默认 `PASSTHROUGH`，确保 IDE/Terminal 绝不被误改。 | §5.4 |
| **AppKit RTF 合成陷阱** | 若只注册 `html` + `rtf` 而不注册 `plainText`，macOS AppKit 会自动从 RTF 合成纯文本导致转换结果泄漏。因此**必须始终显式注册 `public.utf8-plain-text`**（显式 Provider 优先级高于系统合成）。 | §8.3 R1–R2 / 附录 A.10 |
| **源格式保全边界** | 接管需调用 `clearContents()`。合法现代 UTI 与第三方反向 DNS 类型（如 `org.chromium.source-url`）可 100% 直通还原；无法直通的 Legacy Flavor 或多文件复制由前置护栏 G1–G5 拦截并放弃接管。 | §8.9 / 附录 A.7, A.11 |
| **进程退出数据持久化** | 延迟供给的数据驻留在本进程内存，进程退出后未物化的剪贴板会变为空（`types=[]`）。通过**生命周期临终同步物化（`applicationWillTerminate` + 信号钩子 + Panic Hook）与首次拉取后惰性缓存**彻底解决，避免使用 1s 定时器重写引发粘贴期竞态。 | §8.8 / 附录 A.8 |
| **回调耗时防线** | Provider 回调运行在宿主粘贴上下文中，**严禁调用 L2 OCR 或 L3 AI**，仅允许 L1 内存计算，并受 **护栏 G5（源文本 `≤256KB`）+ `8ms` 硬超时降级** 双重保护。 | §8.11 |

### 3.5 权限分层模型：Tier 0 与 Tier 1

为了兼顾“开箱即用零权限弹窗”与“进阶极客自动化体验”，所有系统能力按权限要求严格划分为两层：

| 能力模块 | Tier 0（**默认激活，0 项系统权限**） | Tier 1（**显式开启，需 1 项 Accessibility 权限**） |
|---|---|---|
| **自动挡主链路（§4.1）** | ✅ 完整可用（`NSPasteboard` 读写无需任何 TCC 权限） | — |
| **`⌘C⌘C` 双击复制呼出 HUD** | ✅ 完整可用（基于外部复制事件差分 + 同 Hash + 同 SourceApp 检测，零钩子） | ✅ 可选开启 `CGEventTap` 硬件级按键监听（需 Input Monitoring，非默认） |
| **`⌃⌥V` / `⌥⇧V` 全局热键** | ✅ 完整可用（Carbon `RegisterEventHotKey` / Win32 `RegisterHotKey` 无需 TCC 权限） | — |
| **Micro-HUD 弹出坐标定位** | ✅ 紧贴当前鼠标指针位置 `NSEvent.mouseLocation` 偏移 `(12px, 12px)` | ✅ 紧贴精确文本闪烁光标（`AXUIElement` `kAXBoundsForRangeParameterizedAttribute`） |
| **HUD / 抽屉转换结果上屏** | ✅ 原子写回系统剪贴板，HUD 提示 `已复制，请按 ⌘V 粘贴` | ✅ 写回剪贴板后通过 `enigo`（`CGEventPost`）自动向目标窗口注入单次 `⌘V` |
| **L3 打字机流式分块注入** | ❌ 不启用（统一走 HUD 内流式预览 + 原子粘贴） | ✅ 可选开启，须受前台 PID 逐块校验等三条安全栏约束（§5.3） |
| **目标应用分支模式（§3.6）** | ❌ 不启用 | ✅ 可选开启（结合焦点窗口识别与预渲染 O(1) 查表） |

**`⌘C⌘C` 的 Tier 0 零权限判定状态机（三条件必须同时满足）：**

1. **外部复制事件间隔 `< 250ms`**：排除 PasteFlow 自身接管或物化写入（通过 `LoopGuard` 过滤）后，连续两次外部源剪贴板变更的时间间隔小于 `250ms`。
2. **内容哈希完全一致**：前后两次外部复制内容的 `BLAKE3 contentHash` 相等。
3. **复制源应用完全一致**：前后两次外部复制的 `SourceApp`（`bundleId` / `processName`）相等。

> **状态机协同铁律：** `DoubleCopyDetector` 必须挂载在 `ClipboardOwner` 所有权丢失判定之前。当用户在 `250ms` 内按下第二次 `⌘C` 时，源应用再次调用 `clearContents()` 会覆盖第一次接管的 Provider；此时系统识别为双击事件，**一方面立即对第二次复制重新执行自动挡接管（Re-own），确保即使用户按 `Esc` 关闭 HUD 后直接按 `⌘V` 依然能享受自动挡；另一方面呼出 `Micro-HUD` 并取消待弹出的自动挡 Toast。**

### 3.6 可选进阶模式：目标应用分支 (Opt-in Auto-Pilot Pro, v0.2.0+)

针对极少数必须在同一纯文本通道向不同目标应用分发不同内容的场景，在 v0.2.0 提供默认关闭的 Tier 1 进阶模式：

* **复制时预计算 + 粘贴时 O(1) 查表**：复制瞬间在后台线程为各候选 `TargetApp` 预渲染结果并存入内存哈希表 `(contentHash, targetApp)`。
* **极速钩子响应**：粘贴钩子内仅执行「获取前台窗口 → 内存 O(1) 查表 → 单次剪贴板写入」并立即返回，禁止任何实时解析或 I/O。
* **钩子健康自检（Hook Health Monitor）**：定期检测低阶钩子是否被系统静默卸载，支持自动重装并在诊断面板（§11.4）实时暴露状态。

## 4. 核心交互形态与键盘状态机 (Interaction & Input State Machine)

### 4.1 交互面一：场景感知“自动挡” (Silent Auto-Pilot)

* **触发条件**：用户在任意应用执行原生 `Cmd/Ctrl + C`，随后在目标应用执行原生 `Cmd/Ctrl + V`。全程不拦截任何按键。
* **标准执行时序**：
  1. **源应用事件驱动捕获（Capture）**：通过订阅系统级窗口激活通知（macOS `NSWorkspace.didActivateApplicationNotification` / Windows `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)`），在内存中维护容量为 16 的 `(bundleId, processName, timestamp)` 环形缓冲区。当检测到外部 `changeCount` 自增时，取时间戳早于或等于自增时刻的最新一条记录作为 `SourceApp`（若间隔超过 `500ms` 则标记 `resolutionConfidence = 'stale'`，仅允许匹配通配符 `'*'` 的规则）。
  2. **护栏校验与接管（Guard & Own）**：依次执行隐私黑名单过滤（§8.7）与接管前置护栏 G1–G5 校验（§8.9）。全部通过且命中激活滤镜（置信度 `≥ minConfidence`）时，调用 `FormatPreserver` 缓存源格式并注册 `NSPasteboardItemDataProvider` 接管剪贴板。
  3. **260ms 消抖微反馈（Debounced Toast）**：接管完成后启动 `260ms` 消抖定时器（跨过 `250ms` 的 `⌘C⌘C` 双击检测窗口）。若窗口期内未发生第二次复制，则在鼠标/光标右下角浮现无焦点毛玻璃胶囊 Toast（停留 `800ms` 后自动淡出）；若窗口期内触发了 `⌘C⌘C`，则直接取消 Toast 并弹出 `Micro-HUD`，彻底避免两个浮窗在同位置重叠。
     * **动态 Toast 文案规范（按生效的 `plainTextPolicy` 分支渲染）**：
       * `PASSTHROUGH`：`✨ 已就绪 · 富文本应用将贴为排版格式 · 纯文本应用保持原文`
       * `CLEANED`：`✨ 已自动净化剪贴板内容（双击 ⌘C 可贴回原文）`
       * `SANITIZED_FORCED`：`🛡️ 已自动脱敏敏感凭证（原文回贴已锁定）`
  4. **按需供给（Lazy Provision）**：用户在目标应用按下 `Cmd/Ctrl + V` 时，Provider 回调在 `8ms` 预算内执行对应通道的 L1 转换并返回数据，随后在后台异步将转换产物回写至 SQLite（置 `channels.*.status = 'ready'`）。

### 4.2 交互面二：光标微胶囊“手动挡” (Cursor Micro-HUD)

* **触发通道**：
  * **通道 A（源端双击复制，默认主推，Tier 0）**：选中内容后按住 `Cmd/Ctrl` 连续双击 `C` 键（间隔 `< 250ms`）。
  * **通道 B（目标端快捷键呼出，备选，Tier 0）**：macOS 按 `⌃⌥V`（`Control + Option + V`）；Windows 按 `Ctrl + Alt + V`。
* **UI 形态与定位**：
  * 弹出宽 `260px` 的高颜值毛玻璃微胶囊菜单，最多展示 4 个基于当前 `ContentTags` 动态推荐的动作项，底部固定包含「粘贴原始内容」项（当条目命中 `contains_secret` 时，该项置灰并显示 `🔒 含敏感信息，原文回贴已禁用`）。
  * **坐标定位**：Tier 1 优先通过 Accessibility API 获取精确文本闪烁光标（Caret）坐标；Tier 0 或获取失败时回退至当前鼠标指针偏移 `(12px, 12px)` 处；鼠标越界时回退至活动窗口正中偏下位置。
* **跨平台无焦点键盘路由**：
  * **macOS**：采用 `NSPanel` + `NSWindowStyleMaskNonActivatingPanel`，在不激活 PasteFlow 进程、不抢夺目标窗口焦点的前提下成为 Key Window，直接接收数字键 `1~4`、`Enter`、`Esc` 与方向键。
  * **Windows**：采用 `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW`。因 Win32 下非激活窗口无法获得键盘焦点，仅在 HUD 可见期间临时安装低阶键盘钩子捕获 `1~4` / `Enter` / `Esc` 转发给 HUD 状态机，HUD 隐藏瞬间立即卸载钩子。
* **极速原子上屏与 L3 预览**：
  * **L1 / L2 本地转换**：用户触发动作后，结果一次性写入剪贴板。Tier 1 自动注入单次 `Cmd/Ctrl + V` 上屏；Tier 0 显示 `✅ 已写入剪贴板，请按 ⌘V` 微提示。
  * **L3 AI 流式转换**：微胶囊就地展开流式预览卡片（首块响应 `< 600ms`，实时滚动展示生成文本，支持随时按 `Esc` 中止），生成完毕后默认执行**一次性原子粘贴**，避免流式按键注入打碎目标应用撤销栈或因中途切窗导致跨窗口数据泄漏。

### 4.3 交互面三：磁吸式“历史与贪吃蛇”抽屉 (History & Stack Drawer)

* **触发方式**：快捷键 `⌥⇧V`（macOS）/ `Alt + Shift + V`（Windows），或鼠标停靠屏幕边缘触发。
* **模块 A — 时间轴历史（Timeline）**：
  * 基于 SQLite FTS5 提供毫秒级全文检索、按类型过滤（文本 / 图片 / 链接 / 代码 / 文件）与常用项置顶（Pin）。
  * **延迟落盘规范**：复制入库瞬间仅写入 `original` 原始内容，各转换通道状态初始化为 `status: 'pending'`；仅当某通道被消费方实际拉取后，才异步回写转换结果并置为 `'ready'`。若用户在通道未被拉取前点开卡片的 Before/After 预览，由前端预览逻辑临时调用 L1 现算展示，不回写污染 `channels` 的真实消费状态。
* **模块 B — 贪吃蛇收集槽（Stack Mode / Basket）**：
  * 通过抽屉顶部开关或全局热键 `⌥⇧S`（macOS）/ `Alt + Shift + S`（Windows）一键开启。
  * 开启后，连续 N 次 `Cmd/Ctrl + C` 按序压入队列 `StackQueue[1..N]`，屏幕角落显示微型无焦点计数角标 `[🐍 已收集 5 项]`。
  * **三种一键消费模式**：
    1. **换行拼接**：用换行符或自定义分割线合并全部片段并一次性原子粘贴。
    2. **去重编号**：自动剔除重复片段并生成 `1. 2. 3.` 有序列表。
    3. **AI 聚合整理（核心差异化能力）**：将 N 个零散片段作为上下文输入 L3 模型，直接输出结构化会议纪要、竞品对比表或待办清单。

### 4.4 交互面四：“滤镜商店”主面板与 10 秒破冰 (Main Dashboard & Onboarding)

* **10 秒角色破冰（First-Run Onboarding）**：
  * 首次启动无任何系统权限弹窗，仅展示四张角色卡片供多选：💼 **日常办公与写作** | 📊 **财务与数据运营** | 🎓 **学生与学术研究** | 💻 **开发者与极客**。
  * 角色选择决定默认激活的滤镜组合及模糊场景下的 `plainTextPolicy` 初值（勾选 💻 开发者时纯文本通道优先保持源码直通）。
* **可视化魔法滤镜卡片**：
  * 每张卡片包含：滤镜名称、场景图标、Before/After 转换对比预览、`Auto / Manual / Off` 三态开关、`plainTextPolicy` 单滤镜策略下拉框（`PASSTHROUGH` / `CLEANED`，`SANITIZED_FORCED` 滤镜锁定不可改）以及 `SourceApp` 源应用过滤配置。
* **自然语言造滤镜（AI Pipe Builder，v0.2.0）**：
  * 用户输入一句话需求（如“把复制的杂乱收货地址提取出姓名、电话、省市区，并用 Tab 分隔”），系统自动生成符合 Schema 的 `PipeRecipe` JSON 并挂载至微胶囊。

### 4.5 快捷键全局登记表 (Global Hotkey Registry)

本表为全局快捷键的唯一真源，代码中须收敛为单一常量表（`src/core/hotkeys.rs`），并由单元测试断言任意两个激活快捷键无冲突：

| 功能模块 | macOS 键位 | Windows 键位 | 权限层级 | 底层检测机制 | 冲突核查与约束 |
|---|---|---|---|---|---|
| **自动挡主链路** | 原生 `⌘C` / `⌘V` | 原生 `Ctrl+C` / `Ctrl+V` | Tier 0 | 剪贴板序列号监听，**不拦截按键** | 零按键占用，与系统及第三方软件 100% 兼容 |
| **HUD 呼出 · 通道 A（主推）** | `⌘C` 双击（`<250ms`） | `Ctrl+C` 双击（`<250ms`） | Tier 0 | 外部复制事件差分 + 同 Hash + 同 SourceApp | 复用原生复制键，幂等零副作用 |
| **HUD 呼出 · 通道 B（备选）** | `⌃⌥V` | `Ctrl+Alt+V` | Tier 0 | `RegisterEventHotKey` / `RegisterHotKey` | 避开 macOS `⌥V`（输出 `√`）与 Windows Office `Alt+V` |
| **历史与贪吃蛇抽屉** | `⌥⇧V` | `Alt+Shift+V` | Tier 0 | `RegisterEventHotKey` / `RegisterHotKey` | 与 HUD 通道 B 完全独立，无键位重叠 |
| **贪吃蛇模式开关** | `⌥⇧S` | `Alt+Shift+S` | Tier 0 | `RegisterEventHotKey` / `RegisterHotKey` | 独立键位，支持自定义修改 |
| **HUD 内部动作导航** | `1`–`4` / `↩` / `Esc` | `1`–`4` / `↩` / `Esc` | Tier 0 | macOS：`NSPanel` 直接收键<br>Windows：HUD 可见期临时钩子转发 | Windows 侧钩子必须在 HUD 隐藏瞬间立即卸载 |
| **永久禁用键位（黑名单）** | ~~`⌥V`~~ / ~~`⌘V⌘V`~~ | ~~`Alt+V`~~ / ~~`Ctrl+V+V`~~ | — | — | **严禁在任何版本中启用**（存在字符映射冲突或日常粘贴延迟） |

## 5. 三级引擎能力矩阵与通道策略 (3-Tier Engine Matrix & Channel Policy)

系统严格按 L1 → L2 → L3 三级漏斗路由，严禁将本地确定性算法可解决的任务交由大模型处理。

### 5.1 L1：本地闪电引擎 (Deterministic Local Engine)

* **性能与执行边界**：耗时 `< 5ms`（硬超时 `8ms`）；占日常高频使用 **70%**；100% 离线零成本。仅在消费方拉取通道或 HUD 手动调用时于内存中执行，**handler 内部严禁任何磁盘、网络或数据库写入 I/O**。
* **底层技术实现**：Rust 线性时间正则（`regex` crate，严禁引入带回溯的正则引擎）、Markdown/HTML AST 解析（`pulldown-cmark`、`scraper`）、代码结构推断（`serde_json`）。
* **核心内置滤镜矩阵**：

| 序号 | 滤镜名称 | 核心转换逻辑与目标通道行为 | 默认 `plainTextPolicy` | MVP 排期 |
|---|---|---|---|---|
| **1** | **【AI 星号净化器】** | 将 DeepSeek / ChatGPT / Claude 复制的 Markdown 转换为带 `mso-` 专有样式的 `CF_HTML` / `public.html` 与 `public.rtf` 双通道富文本，贴入 Word / 飞书直接获得原生标题层级、加粗与边框表格；同时配套一个独立的 `MANUAL_ONLY` 配方提供「转微信精致纯文本」HUD 动作。 | **`PASSTHROUGH`**（确保 Terminal / VSCode / IDE 按 `⌘V` 100% 拿到原始 Markdown） | **v0.1.0 [Must]** |
| **2** | **【社媒/电商链接瘦身】** | 自动剥离淘宝、B站、小红书、京东及海外链接中的追踪参数（`spm=`、`vd_source=`、`utm_*` 等）与冗长分享口令，还原纯净直达链接。 | **`CLEANED`**（剥离追踪参数与口令即为纯文本粘贴的核心诉求） | **v0.1.0 [Must]** |
| **3** | **【网页牛皮癣洗白】** | 剥离网页复制携带的刺眼背景色、内联字号、冗余 span 与广告尾巴，仅保留语义化段落、加粗、列表与纯净文本。 | **`CLEANED`** | v0.1.x / v0.2.0 |
| **4** | **【PDF 断行缝合】** | 自动识别中英文 PDF 复制产生的硬回车断行与跨行连字符，智能合并为通顺自然段。 | **`CLEANED`** | v0.1.x / v0.2.0 |
| **5** | **【开发者瑞士军刀】** | JSON 格式化与转 TS / Go / Python / Rust 类型定义；cURL 转 `fetch` 代码；Unix 时间戳、JWT 与 Base64 实时解码。仅通过 HUD 手动触发，绝不自动改写代码。 | **`PASSTHROUGH`** | v0.1.x / v0.2.0 |
| **6** | **【DLP 隐私脱敏盾】** | 自动识别 API Key（`sk-...`、`ghp_...`）、数据库连接串、PEM 私钥、手机号、身份证号并替换为 `<REDACTED>`。置信度门控设为 `0.5`（宁可误脱敏不可漏脱敏）。 | **`SANITIZED_FORCED`**（三通道强制脱敏，锁定不可改，禁用原文回贴） | **v0.1.0 [Must]**（基础规则） |

### 5.2 L2：系统原生视觉引擎 (Native OS Vision Engine)

* **性能与执行边界**：热态耗时 `< 100ms`（macOS 实测 `52–70ms`）；占日常使用 **20%**；100% 离线、零安装包膨胀。排期归属 **v0.2.0**。
* **硬性工程约束**：
  1. **严禁进入 Provider 回调**：macOS Vision 机器级首次调用因模型资产物化耗时高达 **13,554ms**（附录 A.2），若在 `provideDataForType` 内同步调用会直接挂死宿主应用的 `⌘V`。L2 只能通过**「应用启动后台预热 + 复制图片后异步预提取缓存 + HUD 手动触发」**运行。
  2. **强制使用 `.accurate` 识别级别**：实测 `.fast` 模式虽耗时 50ms，但会漏识别 40% 的单元格文本块（附录 A.2）。
  3. **Windows 语言包探测与降级**：`Windows.Media.Ocr` 依赖系统 `Language.OCR*` 能力包，启动时必须探测 `AvailableRecognizerLanguages`；若目标语言缺失，在 UI 中给出系统语言包安装指引，不内置体积膨胀的离线模型（D4 决议）。
* **核心内置滤镜**：
  1. **【截图秒变 Excel】**：采用**「表格框线检测优先 + 二维坐标聚类兜底」**双层策略——优先通过 `VNDetectContoursRequest` / 形态学横竖线提取获取精确单元格边界；无框线表格则执行 **y 轴重叠容差聚类分行 + 全局 x 轴间隙投影分列 + 跨列合并块二次切分**（解决附录 A.3 实测发现的同行 y 值偏差与跨列文本合并问题），输出 TSV 与 HTML Table。
  2. **【离线极速 OCR】**：截图或扫描件一键提取为纯净可编辑文本。
  3. **【二维码与色板提取】**：自动解析剪贴板图片中的二维码链接及主视觉 Hex / Tailwind 色卡。

### 5.3 L3：流式 AI 与 Agent 引擎 (Streaming AI & MCP Engine)

* **性能与执行边界**：HUD 首块可见反馈 `< 600ms`；占日常使用 **10%**；支持 BYOK（自备 API Key）与本地 Ollama，不作免费云端额度承诺。
* **上屏安全准则**：
  * **默认原子粘贴**：流式生成过程在 `Micro-HUD` 气泡内实时滚动渲染，生成完成后一次性写入剪贴板并执行单次粘贴。
  * **Opt-in 打字机流式注入的三条强制安全栏（仅 Tier 1 可开启，缺一不可）**：
    1. **前台 PID 逐块重校验**：每注入一个分块前校验当前前台应用 PID，若与流式启动时的 PID 不一致（用户中途 `⌘Tab` 切窗），**立即中止注入并清空待输出缓冲**，防止跨窗口数据泄漏。
    2. **撤销栈风险显式告知**：在设置开关处明确提示“逐块流式注入会拆分目标应用的 `⌘Z` 撤销栈”。
    3. **任意键即时熔断**：HUD 显示醒目停止按钮，且用户按下任意键（尤其是 `Esc`）立即中止注入，剩余完整文本自动保留在剪贴板中。
* **核心内置滤镜**：
  1. **【得体打工人滤镜】**（v0.1.0 Must）：将大白话或情绪化草稿润色为得体专业的职场邮件或 IM 回复。
  2. **【贪吃蛇智能汇总】**（v0.1.0 Must，核心差异化点）：将收集槽中的多条零散片段提炼为结构化纪要、对比表或待办清单。
  3. **【跨语境地道翻译】**（v0.2.0）：外文段落译为母语，或中文描述一键转为规范英文 Git Commit Message。
  4. **【UI 截图转代码 / 公式识别】**（v0.2.0）：调用多模态模型将截图还原为 LaTeX 公式或 Tailwind 组件。
  5. **【MCP 上下文桥接】**（v0.2.0）：作为本地 MCP Server 向 Cursor、Claude Desktop、Trae 暴露经 DLP 脱敏后的剪贴板历史与收集槽资源。

### 5.4 `plainTextPolicy` 纯文本通道策略规范

由于系统剪贴板的 `public.utf8-plain-text` / `CF_UNICODETEXT` 通道在同一时刻只能承载一份内容，且若不显式注册该通道，macOS AppKit 会自动从 `public.rtf` 合成纯文本导致转换结果泄漏（附录 A.10），系统通过以下三级策略枚举精确控制纯文本通道行为：

| 策略枚举值 | 通道输出行为 | 适用准则 |
|---|---|---|
| **`PASSTHROUGH`** | 显式注册 `plainText` 通道并**原样输出用户复制的原始文本** | 原始纯文本本身即为高价值干净源码（如 Markdown、代码、JSON）。宁可让纯文本软件少一次美化，绝不破坏 IDE / Terminal 中的代码。 |
| **`CLEANED`** | 显式注册 `plainText` 通道并**输出清洗/修复后的纯文本** | 原始纯文本本身存在明显缺陷（如带追踪参数的购物口令、PDF 断行、夹带广告的网页文本），用户粘贴纯文本的核心目的就是获取清洗结果。 |
| **`SANITIZED_FORCED`** | **`html` / `rtf` / `plainText` 三通道全部强制输出脱敏结果**，且运行时强制将 `preserveOriginal` 置为 `false`、禁用原文回贴 | 内容命中 `contains_secret` 敏感凭证。属于最高优先级安全红线，任何用户设置或社区配方均不可覆盖。 |

**运行时策略优先级解析链（由高到低）：**

```
1. SANITIZED_FORCED（只要 ContentTags 含 contains_secret，无条件强制锁定）
       ▲ 覆盖
2. 用户在「滤镜商店」单张滤镜卡片上显式指定的策略（PASSTHROUGH / CLEANED）
       ▲ 覆盖
3. PipeRecipe 配方 JSON 中声明的 output.plainTextPolicy 字段
       ▲ 覆盖
4. 破冰角色卡设定的模糊默认值（💻 开发者角色优先 PASSTHROUGH）
       ▲ 覆盖
5. 系统全局安全兜底默认值 = PASSTHROUGH（防止第三方未声明策略的社区配方静默篡改用户纯文本）
```

## 6. 技术栈与系统架构规范 (Technical Architecture & Stack)

### 6.1 跨端桌面框架与多窗口管理

* **核心框架**：**Tauri 2.0**（严禁使用 Electron，确保安装包 `< 10MB`）。
* **最低系统版本要求（Minimum Deployment Target）**：macOS `13.0+`（Ventura，满足 `VNRecognizeTextRequest` 中文识别要求）；Windows `10 1809+`。
* **多窗口架构（基于稳定版独立 `WebviewWindow` API，不使用 `unstable` multiwebview 特性）**：
  1. **`micro-hud`（光标微胶囊）**：应用启动后延迟 `2s` 在后台预热创建，保持 `visible: false` **常驻内存不销毁**（D1 决议），确保 `⌘C⌘C` 触发时实现 `0ms` 瞬时唤醒。
  2. **`drawer`（历史与贪吃蛇侧边抽屉）**：独立 `WebviewWindow`，支持边缘停靠与快捷键秒开。
  3. **`main-dashboard`（滤镜商店与设置主面板）**：低频重型视图，采用**按需创建、关闭后延迟 `30s` 销毁释放 Webview 进程**的策略，最大化节约日常内存。

### 6.2 Rust 后端核心模块与技术选型

| 模块 / 能力域 | 选定技术方案 | 选型依据与工程规范 |
|---|---|---|
| **剪贴板基础读写与监听** | `clipboard-rs` (`0.3.5+`) + **Vendor/Fork 锁定** | `arboard` 不支持 HTML/RTF，无法用于多通道分发。`clipboard-rs` 支持全格式与 Watch，但须 Vendor 到仓库内以防上游断更或企业微信等畸形 HTML 引发崩溃。 |
| **剪贴板持有与延迟供给** | **自研 `ClipboardOwner`**：macOS 基于 `objc2` (`NSPasteboardItemDataProvider`)；Windows 基于 `windows-rs` (`OleSetClipboard` + `IDataObject`) | 现有开源 Crate 均未封装延迟供给回调，必须基于平台原生 FFI 自研。 |
| **源格式保全与护栏** | **自研 `FormatPreserver`** | 负责接管前枚举源 `types`、执行 Legacy 别名正则过滤、判定 G1–G5 护栏并缓存合法直通格式。 |
| **生命周期临终物化** | **自研 `Materializer`** | 监听 `applicationWillTerminate`、Unix 退出信号（`SIGTERM`/`SIGINT`/`SIGHUP`）及 Rust Panic Hook，在进程退出前校验所有权并将延迟通道物化为真实剪贴板数据。 |
| **回调超时预算控制** | **自研 `CallbackBudget`** | 为 Provider 回调提供 `8ms` 硬超时保护、Panic 捕获（`std::panic::catch_unwind` across FFI 边界）与原始数据兜底返回。 |
| **零权限双击复制检测** | **自研 `DoubleCopyDetector`** | 基于外部 `changeCount` 差分（`<250ms`）+ 同 `contentHash` + 同 `SourceApp` 实现零权限 `⌘C⌘C` 识别。 |
| **复制源进程事件感知** | `active-win-pos-rs` + 原生窗口激活事件流 | 订阅 `didActivateApplicationNotification`（macOS）/ `SetWinEventHook`（Windows）维护 16 槽时间戳环形缓冲，精确回溯复制瞬间的 `SourceApp`。 |
| **输入光标定位与回退** | **自研 `CaretLocator`** | Tier 1 调用 `AXUIElement` / `UIAutomation` 获取精确 Caret；Tier 0 或失败时执行鼠标坐标与窗口中心三级回退（§8.4）。 |
| **全局热键与模拟注入** | `tauri-plugin-global-shortcut` + `enigo` (`0.6.1+`) | 彻底移除停更的 `rdev`。`enigo` 仅在 Tier 1 自动上屏时调用。 |
| **本地存储与全文检索** | `rusqlite` (`0.40.2+`，开启 SQLite3 FTS5) | 存储历史记录、通道状态与本地图片缩略图缓存。 |

### 6.3 前端视图层技术栈

* **核心栈**：`TypeScript` + `React 19` + `Vite` + `Tailwind CSS` + `shadcn/ui` + `Framer Motion`（用于微胶囊与抽屉的丝滑过渡动画）。

### 6.4 系统全局数据流转图 (Master Data Flow Pipeline)

```
[System Clipboard Event: 用户按下原生 Cmd/Ctrl+C]
       │
       ▼
[1. LoopGuard & Ownership Tracker]
       ├─► 匹配 RecentInternalWrites (BLAKE3 Hash / x-pasteflow-internal / 物化写入) ──► [短路忽略]
       │
       ▼
[2. SourceApp Event Resolver & PrivacyGuard (§8.7)]
       ├─► 从 didActivateApplication 环形缓冲回溯精确 SourceApp
       ├─► SourceApp ∈ 密码管理器黑名单 或 携带 ConcealedType / ExcludeClipboard 标记 ──► [彻底丢弃，不进内存不落盘]
       │
       ▼
[3. DoubleCopyDetector (§3.5)]  ← 挂载于所有权覆盖判定之前
       ├─► 满足 (外部复制间隔 <250ms && 同 contentHash && 同 SourceApp)
       │     └──► 标记「触发 Micro-HUD 手动挡」+ 取消待弹出的自动挡 Toast + 继续向下完成重接管(Re-own)
       │
       ▼
[4. Content Analyzer & SQLite FTS5 Storage]
       ├─► 提取 ContentTags (markdown / dirty_html / url_with_tracker / contains_secret 等)
       ├─► 异步写入 SQLite: 仅落盘 original 原文，channels.*.status 全部置为 'pending'
       │
       ▼
[5. Ownership Guards G1–G5 (§8.9) & Filter Matcher]
       ├─► 任一护栏未通过 (多 item / 含文件 URL / 纯 Legacy 格式 / 文本 >256KB / 直通缓冲 >4MB)
       │     或 未命中任何激活 Auto 滤镜 / 置信度 < minConfidence
       │     └──► [放弃接管，完全保持系统原生剪贴板] + 记录原因至本地诊断面板 (§11.4)
       │
       ▼
[6. ★ OWN THE PASTEBOARD (持有剪贴板，零转换计算) ★]
       ├─► FormatPreserver 缓存源剪贴板合法直通格式 (排除 Legacy 别名正则)
       ├─► 注册 DataProvider 声明: public.html + public.rtf + public.utf8-plain-text(必选) + 直通 UTI
       ├─► 记录 ownership.changeCountAtOwn
       ├─► 启动 260ms 消抖定时器:
       │     ├─► 若 250ms 内未触发 ⌘C⌘C ──► 弹出按 plainTextPolicy 分支的预告 Toast (800ms 淡出)
       │     └─► 若触发了 ⌘C⌘C         ──► 抑制 Toast，在光标/鼠标处弹出 Non-Activating Micro-HUD
       │
       │   ...... (用户在任意目标应用按下原生 Cmd/Ctrl+V，按键不被拦截) ......
       ▼
[7. Lazy Provision Callback (宿主请求某通道时同步触发)]
       │  ⚠️ 硬性护栏 (§8.11): FFI catch_unwind + 8ms 硬超时；严禁 L2/L3 与任何 I/O
       ├─► 请求 html       ──► [L1 引擎 <5ms] 渲染带 mso- 样式的富文本 ──► 返回并缓存至内存
       ├─► 请求 rtf        ──► [L1 引擎 <5ms] 渲染标准 RTF           ──► 返回并缓存至内存
       ├─► 请求 plainText  ──► [按 §5.4 plainTextPolicy 返回原文 / 净化文本 / 脱敏文本]
       ├─► 请求直通私有 UTI ──► [FormatPreserver 原样返回接管时缓存的源字节流]
       └─► 超时 / Panic / 异常 ──► [立即回退返回原始内容] + 写入诊断面板
       │
       ▼
[8. 异步状态回写与生命周期临终物化 (§8.8)]
       ├─► 通道首次供给完成后，后台异步回写 SQLite (channels.<type>.status = 'ready')
       └─► 当进程收到退出信号 (applicationWillTerminate / SIGTERM / SIGINT / Panic Hook):
             └──► 校验 changeCount == changeCountAtOwn ──(成立)──► 将全部声明通道急切物化为真实剪贴板数据后退出
```

## 7. 核心数据结构与协议定义 (Core Data Schemas)

### 7.1 剪贴板数据条目 (`ClipboardItem`)

```typescript
export interface ClipboardItem {
  id: string;                  // UUID v7 (按时间有序)
  createdAt: number;           // Unix Timestamp (ms)
  contentHash: string;         // BLAKE3 Hash (用于去重、预渲染缓存键与 ⌘C⌘C 双击判定)
  primaryType: 'text' | 'html' | 'rtf' | 'image' | 'files';

  /** 原始剪贴板数据（复制入库瞬间立即写入） */
  original: {
    plainText?: string;
    html?: string;
    rtf?: string;
    imagePath?: string;        // 本地缩略图/原图缓存路径
    filePaths?: string[];
  };

  /**
   * 延迟供给通道产物：
   * 复制入库时所有通道 status 均初始化为 'pending'；
   * 仅当通道首次被消费方拉取后，异步回写 data 与 providedAt 并置为 'ready'。
   */
  channels?: {
    html?:      { status: 'pending' | 'ready' | 'failed'; data?: string; pipeId?: string; providedAt?: number };
    rtf?:       { status: 'pending' | 'ready' | 'failed'; data?: string; pipeId?: string; providedAt?: number };
    plainText?: { status: 'pending' | 'ready' | 'failed'; data?: string; pipeId?: string; providedAt?: number };
  };

  /** 源剪贴板格式清单与直通保全状态（§8.9） */
  sourceFormats: {
    declaredTypes: string[];       // 接管前源应用声明的全部 pasteboard type
    itemCount: number;             // pasteboardItems.count (>1 触发护栏 G2)
    passthroughBytes: number;      // 直通缓存总字节数 (>4MB 触发护栏 G5)
    passthroughTypes: string[];    // 成功注册直通 provider 的合法类型
    droppedLegacyTypes: string[];  // 被过滤的 legacy 别名
    preservationComplete: boolean; // 是否满足完整保全 (false 则依护栏 G4 放弃接管)
  };

  /** 复制源上下文（基于窗口激活事件流回溯，§4.1） */
  sourceContext: {
    appName: string;               // e.g., "Google Chrome", "Cursor"
    bundleId?: string;             // e.g., "com.google.Chrome"
    processName: string;           // e.g., "chrome.exe"
    windowTitle?: string;          // e.g., "DeepSeek - 探索未至之境"
    resolutionConfidence: 'exact' | 'stale'; // 事件时间差 >500ms 标记为 'stale'
  };

  tags: Array<
    | 'markdown'
    | 'dirty_html'
    | 'url_with_tracker'
    | 'pdf_broken_lines'
    | 'json'
    | 'curl'
    | 'code'
    | 'contains_secret'
    | 'table_image'
  >;

  /** 当前条目最终生效的纯文本通道策略（由 §5.4 优先级链解析得出） */
  plainTextPolicy: 'PASSTHROUGH' | 'CLEANED' | 'SANITIZED_FORCED';

  isPinned: boolean;
  inStack: boolean;                // 是否处于贪吃蛇收集槽队列中

  /** 剪贴板所有权与临终物化状态（§8.2 / §8.8） */
  ownership: {
    owned: boolean;                // 当前是否仍持有系统剪贴板
    changeCountAtOwn: number;      // 接管时的系统 changeCount / 序列号
    lostAt?: number;               // 所有权被外部应用夺走的时间戳
    materialized: boolean;         // 是否已在退出前物化为真实剪贴板数据
    materializedAt?: number;       // 物化完成时间戳
    materializeChangeCount?: number; // 物化写入后的新 changeCount
  };

  /** 安全锁定标记：当命中 contains_secret 时为 true，强制禁用原文回贴（§8.7） */
  originalReplayDisabled: boolean;
}
```

### 7.2 声明式滤镜配方协议 (`PipeRecipe`)

所有内置及社区贡献的滤镜配方均遵循此结构（存放于 `pipes/*.json`），JSON Schema 强制开启 `additionalProperties: false`，严禁在 JSON 内夹带任何非协议批注字段：

```json
{
  "$schema": "https://raw.githubusercontent.com/PasteFlowApp/pasteflow/main/schema/pipe-v1.json",
  "id": "builtin.markdown-to-office",
  "name": "AI 星号净化器 (Markdown 转办公格式)",
  "description": "将 AI 助手复制的带 # 和 ** 的文本，自动转换为 Word/飞书原生加粗与标准表格",
  "author": "PasteFlow Official",
  "version": "1.3.0",
  "icon": "Sparkles",
  "category": "office",
  "tier": "L1_LOCAL",
  "trigger": {
    "mode": "AUTO_AND_MANUAL",
    "matchContentTags": ["markdown"],
    "matchSourceApps": ["*"],
    "ownChannels": ["html", "rtf", "plainText"],
    "matchTargetApps": null,
    "minConfidence": 0.85
  },
  "processor": {
    "engine": "builtin_rust_handler",
    "handlerName": "markdown_to_rich_office",
    "fallbackPrompt": null,
    "providerTimeoutMs": 8
  },
  "output": {
    "channels": {
      "html": { "handler": "markdown_to_cf_html" },
      "rtf": { "handler": "markdown_to_rtf" },
      "plainText": { "handler": "identity" }
    },
    "plainTextPolicy": "PASSTHROUGH",
    "preserveOriginal": true,
    "toastMessages": {
      "PASSTHROUGH": "✨ 已就绪 · 富文本应用将贴为排版格式 · 纯文本应用保持原文",
      "CLEANED": "✨ 已自动净化剪贴板内容（双击 ⌘C 可贴回原文）",
      "SANITIZED_FORCED": "🛡️ 已自动脱敏敏感凭证（原文回贴已锁定）"
    }
  }
}
```

**`PipeRecipe` 核心字段规范表**

| 字段路径 | 类型与必填 | 规范说明 |
|---|---|---|
| `trigger.ownChannels` | `string[]` (必填) | 声明本滤镜接管的输出通道。**铁律：只要包含 `html` 或 `rtf`，就必须同时包含 `plainText`**，防止 AppKit 自动从 RTF 合成纯文本导致泄漏。 |
| `trigger.matchTargetApps` | `string[] \| null` | 默认架构下固定为 `null`；仅在开启 §3.6 目标应用分支（Tier 1）时生效。 |
| `trigger.minConfidence` | `number` (`0.0–1.0`) | 触发自动挡接管的最低特征置信度（常规滤镜建议 `0.85`；DLP 隐私脱敏滤镜设为 `0.5`）。 |
| `processor.providerTimeoutMs` | `integer` (`1–8`) | Provider 回调硬超时毫秒数，默认 `8`，Schema 强制 `maximum: 8`，超限配置由运行时按 `8ms` 截断。 |
| `output.plainTextPolicy` | `enum` (必填) | 取值 `PASSTHROUGH` \| `CLEANED` \| `SANITIZED_FORCED`，缺省回退为 `PASSTHROUGH`。 |
| `output.preserveOriginal` | `boolean` (必填) | 是否允许回贴原文。当内容命中 `contains_secret` 时，运行时无条件将其覆盖为 `false`。 |
| `output.toastMessages` | `object` (必填) | 按生效的 `plainTextPolicy` 动态映射接管后（260ms 消抖）展示的预告文案。 |

### 7.3 MCP Server 暴露接口规范 (`MCP Integration`, v0.2.0)

当用户在设置中开启本地 MCP 服务时，向外部桌面 AI Agent（Cursor / Claude Desktop / Trae）暴露以下标准能力：

* **Resources**：
  * `clipboard://latest`：获取当前最新一条剪贴板内容（强制经过 L1 DLP 隐私脱敏处理）。
  * `clipboard://stack`：获取当前“贪吃蛇收集槽”中聚合的所有多模态/文本片段（强制经过 DLP 脱敏）。
  * `clipboard://history?limit=10`：获取最近 N 条复制记录。
* **Tools**：
  * `run_clipboard_pipe(pipe_id, input_text)`：调用 PasteFlow 内置的转换与清洗管道。
  * `push_to_clipboard(content, format)`：允许外部 Agent 将生成结果按指定格式写入系统剪贴板（调用时自动触发 §8.2 所有权变更处理并记入本地审计日志）。

## 8. 关键技术壁垒与避坑指南 (Critical Edge Cases & Engineering Guardrails)

在编写技术设计文档（TDD）和实现代码时，必须逐项落实以下 **11 项系统级工程护栏**：

### 8.1 防剪贴板死循环 (Loop Guard)

* **问题**：内部接管剪贴板、HUD 写回或退出前物化写入时，均会使系统 `changeCount` / 序列号自增，若不识别会再次触发自身的 `Clipboard Watcher`。
* **强制方案**：
  1. 每次内部写回或接管前，将内容 `BLAKE3 Hash` 与预期的新 `expectedChangeCount` 存入内存环形缓冲区 `RecentInternalWrites`，并附带私有格式标识 `application/x-pasteflow-internal`。
  2. Watcher 检测到 `changeCount <= expectedChangeCount`、携带私有标识或匹配 Hash 时立即短路跳过。

### 8.2 剪贴板所有权丢失处理 (Ownership Loss)

* **问题**：用户在其他应用执行新复制时，源应用会调用 `clearContents()` 夺走剪贴板所有权，使我方注册的 DataProvider 失效。
* **强制方案**：
  1. 持续对比系统 `changeCount` 与 `ownership.changeCountAtOwn`。
  2. 检测到外部变动（且非 `⌘C⌘C` 重接管）时，立即标记旧条目 `ownership.owned = false`、记录 `lostAt`、释放旧 Provider 强引用以防内存泄漏，并**严禁尝试夺回剪贴板**。

### 8.3 跨软件粘贴格式协商三铁律 (Clipboard MIME Negotiation Rules)

基于 macOS AppKit 底层实测（附录 A.10 / A.11），声明与供给通道必须遵守以下三条铁律：

1. **R1 — `plainText` 通道必须显式注册，严禁省略**：只注册 `html` + `rtf` 而不注册 `public.utf8-plain-text` 时，AppKit 会自动从 `public.rtf` 合成纯文本，导致转换结果泄漏至 Terminal 与微信。
2. **R2 — 显式 Provider 优先于 AppKit 自动合成**：同时注册 `plainText` / `html` / `rtf` 时，纯文本消费方必然读取我方显式 Provider 的返回值，不被 RTF 合成值覆盖。
3. **R3 — 仅注册现代 UTI，Legacy 别名交由系统自动合成**：`setDataProvider` 会静默拒绝 `NSStringPboardType`、`Apple HTML pasteboard type` 等 Legacy 标识符；只需注册现代 UTI（如 `public.utf8-plain-text`、`public.html`、`public.rtf`），AppKit 会自动追加对应的 Legacy 别名。

### 8.4 输入光标坐标获取与三级回退 (Caret Position Fallback)

1. **第 1 级（Tier 1 精确光标）**：通过 `AXUIElement`（`kAXFocusedUIElement` → `kAXSelectedTextRange` → `kAXBoundsForRangeParameterizedAttribute`）或 Windows `UIAutomation`（`TextPattern.GetCaretRange` / `GUITHREADINFO.ptCaret`）获取精确输入光标坐标。
2. **第 2 级（Tier 0 默认 / 接口失败回退）**：回退至当前鼠标指针屏幕坐标 `NSEvent.mouseLocation` 偏移 `(12px, 12px)` 处，全程不触发任何权限弹窗。
3. **第 3 级（越界兜底）**：若鼠标位于当前活动窗口所在屏幕之外，回退至活动窗口正中央偏下位置。

### 8.5 富文本目标应用兼容性矩阵 (Compatibility Matrix)

兼容性矩阵作为 CI 的一等公民产物维护，每次 Release 必须附带以下矩阵的回归验证结果：

| 目标应用 | 消费通道 | 必过验证断言 |
|---|---|---|
| **Word 2016 / 365 (Mac & Win)** | `HTML`, `RTF` | `h1`–`h3` 原生标题大纲层级、加粗、有序/无序列表、`mso-` 表格实线边框与代码块等宽字体完好 |
| **WPS Office** | `HTML`, `RTF` | 标题层级、列表缩进与表格边框完好 |
| **飞书 / Lark 文档 & 钉钉文档** | `HTML` | 经过编辑器粘贴净化器后，标题、粗体、列表、代码块与表格结构完整保留 |
| **微信桌面版 (WeChat)** | `plainText` | 默认收到无损 Markdown 原文（`PASSTHROUGH`）；通过 HUD 选择「转微信纯文本」后收到去星号精致排版 |
| **Excel / 飞书表格** | `TSV`, `HTML` | 表格粘贴自动正确分列，千分位数字与日期格式不被破坏 |
| **Notion** | `HTML`, `plainText` | 富文本与块级结构正确解析，无多余空行或转义符残留 |
| **VSCode / Terminal / iTerm2** | `plainText` | **回归红线**：按 `⌘V` 必须 100% 得到原始 Markdown / 原始代码，绝不出现 HTML 标签或 RTF 派生文本 |
| **Slack / Discord** | `plainText` | 不得因误注入 HTML 导致消息格式错乱，亦不得读到 RTF 自动合成文本 |
| **Finder / 访达（多文件复制）** | `public.file-url` | **护栏回归**：复制 3 个文件后按 `⌘V` 必须完整粘贴 3 个文件，不被单 Item 压平为 1 个 |
| **宿主退出后粘贴回归** | 全通道 | **物化回归**：复制 → 正常退出 PasteFlow → 在 Word 与 Terminal 中按 `⌘V`，仍能正常粘出对应通道内容 |

### 8.6 与其他剪贴板管理器共存策略 (Coexistence)

1. **启动冲突探测**：启动时检测常见剪贴板工具进程（Maccy、CopyQ、Paste、Raycast Clipboard、Ditto、EcoPaste），若检测到活跃进程，温和提示用户“建议在另一款工具中将 PasteFlow 加入忽略列表或关闭其自动改写功能”。
2. **被动礼让原则**：一旦检测到外部进程夺走剪贴板所有权（§8.2），立即释放 Provider，绝不争夺写回。
3. **系统历史防污染**：Windows 侧对内部过渡写入及命中 `contains_secret` 的条目强制附加 `ExcludeClipboardContentFromMonitorProcessing` 与 `CanIncludeInClipboardHistory = 0`。

### 8.7 隐私安全绝对红线 (Security Blacklist & DLP)

1. **敏感源进程与系统标记黑名单（不可关闭）**：当 `SourceApp` 属于 `1Password`、`Bitwarden`、`KeePass`、`KeePassXC`、`Keychain Access`、`Enpass`，或剪贴板包含 `org.nspasteboard.ConcealedType`（macOS）/ `ExcludeClipboardContentFromMonitorProcessing`（Windows）时，**完全忽略该次复制：不进内存、不落盘、不触发任何管道、不接管剪贴板**。
2. **内容级 DLP 强制锁定（针对普通应用中复制的敏感密钥）**：当内容命中 `contains_secret`（如从终端或编辑器复制了 `sk-...`、私钥、连接串）时：
   * `plainTextPolicy` 强制锁定为 `SANITIZED_FORCED`；
   * `html` / `rtf` / `plainText` 三个通道全部输出 `<REDACTED>` 脱敏结果；
   * `preserveOriginal` 强制置为 `false`，HUD 与抽屉中的「粘贴原始内容」按钮置灰锁定；
   * 严禁将该条目发送至任何 L3 云端大模型接口。

### 8.8 进程退出数据蒸发防护与生命周期临终物化 (Lifecycle Materialization)

* **问题（实测复现见附录 A.8）**：`NSPasteboardItemDataProvider` 供给的数据驻留在 PasteFlow 进程内存中。若进程在未物化状态下退出，系统 `changeCount` 保持不变，但剪贴板 `types` 变为空数组 `[]`，用户在任何应用中按 `⌘V` 均读到 `nil`（粘不出任何内容）。同时，若采用“复制后 1s 定时物化”，不仅抵消了延迟供给优势，还会在 `t=1s` 调用 `clearContents()` 时与用户正在进行的粘贴发生读取竞态，并导致单次复制触发 3 次 `changeCount` 跳变。
* **强制方案（生命周期临终物化 + 消费后缓存）**：
  1. **日常运行期保持纯正 Lazy 状态**：只要 PasteFlow 进程正常运行，不启动任何定时重写定时器，从而保证单次复制仅产生 1 次接管写入，零粘贴竞态。
  2. **通道首次被拉取后内存缓存**：某通道首次触发 Provider 回调计算出结果后，立即存入当前条目的内存缓存；后续同一剪贴板周期内的重复读取直接 O(1) 返回内存缓存。
  3. **进程退出路径无条件同步物化**：在 `NSApplicationDelegate.applicationWillTerminate`（macOS）、`WM_QUERYENDSESSION` / `WM_ENDSESSION`（Windows）、Unix 信号处理器（`SIGTERM` / `SIGINT` / `SIGHUP`）以及 Rust 全局 `std::panic::set_hook` 中挂载同步物化函数：
     * **所有权前置校验（铁律）**：首先检查当前系统 `changeCount == ownership.changeCountAtOwn`；若不相等，说明剪贴板早已被其他应用的新复制接替，**立即放弃物化**，严禁覆盖用户后来的新复制内容。
     * **执行物化写回**：若所有权仍在我方，立即将所有已声明通道的计算结果与直通缓存作为静态真实数据写入系统剪贴板（Windows 调用 `OleFlushClipboard`），并登记至 `LoopGuard` 后安全退出进程。

### 8.9 接管前置护栏与源格式直通 (Ownership Guards G1–G5)

* **问题（实测复现见附录 A.7 / A.11）**：接管剪贴板必须调用 `clearContents()`，会清空源应用写入的全部原始格式（实测单次复制多达 12 个 type）；同时，`setDataProvider` 无法注册 Legacy 4 字符 Flavor，且单 Item 接管会将多文件复制（`pasteboardItems.count > 1`）压平为 1 个文件。
* **强制方案 A — 源格式直通缓存（`FormatPreserver`）**：
  1. 接管前枚举源 `pasteboard.types`（macOS）/ `EnumFormatEtc`（Windows）。
  2. **Legacy 别名过滤判据（严禁使用 `UTType(s) != nil`，因其会误杀 `org.chromium.source-url` 与 `com.microsoft.excel.data` 等未注册第三方合法类型）**：仅通过以下正则排除系统会自动从现代 UTI 合成的已知 Legacy 别名：
     ```regex
     ^(NS\w+PboardType|Apple .+ pasteboard type|NeXT .+ pasteboard type|CorePasteboardFlavorType 0x[0-9A-Fa-f]{8})$
     ```
  3. 当 `primaryType` 为文本/富文本时，跳过伴生的高清预览位图通道（`public.tiff` / `public.pdf`），将剩余合法现代 UTI 与第三方私有类型的原始 `Data` 读入内存直通缓存，并随 DataProvider 一并声明；当消费方请求这些私有类型时原样返回。
* **强制方案 B — 接管前置护栏 G1–G5（满足任一条件即主动放弃接管，保持系统原生剪贴板）**：

| 护栏编号 | 拦截条件 | 拦截理由与工程依据 |
|---|---|---|
| **G1（纯 Legacy 源护栏）** | 源 `types` 中存在 Legacy 标识符，且其对应的现代 UTI（见下方映射表）不在源 `types` 中 | 该 Legacy 格式无法注册 DataProvider，且系统不会自动合成，强行接管会导致粘贴为空（附录 A.7）。 |
| **G2（多条目压平护栏）** | `pasteboardItems.count > 1` | 单 Item 接管会将多条目剪贴板压平为 1 项，导致多文件或多选复制丢失数据（附录 A.11）。 |
| **G3（文件引用护栏）** | 源 `types` 包含 `public.file-url` 或 `NSFilenamesPboardType` | 文件/文件夹复制不存在文本转换语义，且高度依赖系统 Finder 原生句柄。 |
| **G4（保全完整性护栏）** | 直通预检后 `sourceFormats.preservationComplete == false` | 兜底防线：任何无法保证源核心格式完整还原的场景一律不接管（原则 7）。 |
| **G5（载荷体积上限护栏）** | 源文本通道（`plainText` / `html` / `rtf`）`> 256KB`，或直通格式单通道 `> 1MB`、总直通缓存 `> 4MB` | 从源头阻断超长脏 HTML 在主线程回调内引发超过 `5ms` 的慢解析，并防止巨型伴生数据撑爆 `<25MB` 核心内存指标。 |

**G1 判定用 Legacy → 现代 UTI 标准映射表：**

| Legacy 标识符 | 对应现代 UTI |
|---|---|
| `NSStringPboardType` | `public.utf8-plain-text` / `public.text` |
| `Apple HTML pasteboard type` | `public.html` |
| `NeXT Rich Text Format v1.0 pasteboard type` | `public.rtf` |
| `NSFilenamesPboardType` / `CorePasteboardFlavorType 0x6675726C` | `public.file-url` |
| `Apple URL pasteboard type` / `CorePasteboardFlavorType 0x75726C20` | `public.url` / `public.file-url` |
| `NeXT TIFF v4.0 pasteboard type` | `public.tiff` |
| `Apple PNG pasteboard type` | `public.png` |
| `CorePasteboardFlavorType 0x75743136` | `public.utf16-external-plain-text` |

### 8.10 Universal Clipboard / Handoff 跨设备同步防护

* **问题**：macOS 的 `sharingd` 守护进程会读取剪贴板以通过 Handoff 同步至用户的 iPhone / iPad，从而触发我方 Provider 回调。
* **强制方案**：
  1. 首次接管剪贴板时向用户明确说明“转换通道可被 Apple 通用剪贴板同步至你的其他设备”，并提供设置开关。
  2. 当条目命中 `contains_secret` 时，除强制脱敏外，在接管声明中附加 `org.nspasteboard.ConcealedType` 与 `org.nspasteboard.TransientType`，阻断 `sharingd` 将内容上传至 iCloud 同步链路（列入 Step 0 Spike 实测确认项）。

### 8.11 Provider 回调硬超时与 FFI 异常安全 (Callback Budget)

* **强制方案**：
  1. **FFI 边界 Panic 隔离**：Rust 侧实现的 `pasteboard(_:item:provideDataForType:)` 回调外层必须包裹 `std::panic::catch_unwind`。在 Objective-C / COM FFI 栈上发生未捕获的 Rust Panic 是未定义行为（直接导致进程 Abort），捕获后必须立即回退写入原始数据。
  2. **8ms 硬超时与前置限流**：依赖护栏 G5（源文本 `≤ 256KB`）+ Rust `regex` 线性自动机（严禁回溯型正则）确保 99.9% 的 L1 转换在 `< 2ms` 内完成；同时设置 `8ms` 计时门控，一旦超时或转换异常，**立即向 `item` 写入 `original` 原始通道数据并返回**，绝不返回空指针或阻塞宿主应用的 `Cmd/Ctrl+V`。

## 9. 竞品定位与差异化边界 (Competitive Positioning)

### 9.1 核心竞品全景对照表（数据核实基准：2026-10-14）

| 产品 | 平台 | 开源协议 / 属性 | GitHub Stars | 核心能力与 PasteFlow 的竞争关系 |
|---|---|---|---|---|
| **PowerToys Advanced Paste** | Windows | ✅ 开源（微软官方） | **139,281** (PowerToys) | **Windows 侧最大免费对手**：支持 `Win+Shift+V` 呼出、粘贴为纯文本/MD/JSON、本地 OCR、接入云端 AI 与本地 Ollama。但**依赖专用热键**，未在原生 `Ctrl+V` 上做多通道延迟供给，且不支持 macOS。 |
| **Maccy** | macOS | ✅ MIT | **21,923** | macOS 剪贴板历史标杆，纯 Swift 编写。**仅做历史管理，无任何格式清洗、Markdown 转富文本或 AI 转换能力**，印证 macOS 转换型剪贴板赛道为空白。 |
| **CopyQ** | Win / Mac / Linux | ✅ GPL-3.0 | **12,461** | 老牌极客剪贴板，支持复制时触发正则与 JS 脚本。但属于**复制时急切改写**，配置门槛极高，无原生办公富文本保真引擎与内置 OCR。 |
| **Clipy** | macOS | ✅ MIT | **8,912** | 仅提供历史记录与固定文本片段（Snippets），已长期缺乏大版本演进。 |
| **EcoPaste** | Win / Mac / Linux | ✅ Apache-2.0 (Tauri 2) | **7,503** | **最佳同栈技术参考**（Tauri 2 跨平台剪贴板历史工具）。主打历史管理，曾因企业微信畸形 HTML 踩坑，印证了多格式剪贴板护栏（§8.9）的必要性。 |
| **Ditto** | Windows | ✅ 开源 | **7,297** | 内置 "Enable Auto Merge" 连续复制合并（即贪吃蛇收集槽的传统形态），但无 AI 结构化聚合与富文本转换。 |
| **PasteMD** | Windows | ✅ 开源 | **5,370** (<1年达成) | **核心场景验证者**：通过热键调用 Pandoc 将剪贴板 Markdown 转为 Word/WPS 富文本，不到 1 年斩获 5.3k+ Stars，强力验证了「AI Markdown → 办公软件」是中文社区现象级刚需。 |
| **ClipboardFusion** | Windows | ❌ 商业软件 | — | 支持触发器、C# 宏与 `Ctrl+C` 双击触发，验证了双击复制交互的商业成熟度。 |
| **StopTheMadness / PlainText** | macOS / iOS | ❌ 商业软件 / 免费工具 | — | 专注海外 URL 追踪参数清洗，缺乏针对淘宝、小红书、B站等中文社媒口令的深度净化。 |

### 9.2 不可声称与可声称红线清单

* **❌ 严禁在对外物料中声称“首创”的功能点**：
  * 严禁声称“首创剪贴板自动转换”（CopyQ 自 2009 年已支持）
  * 严禁声称“首创 Markdown 粘贴转 Word”（PasteMD 与 PowerToys 已支持）
  * 严禁声称“首创贪吃蛇连续复制收集槽”（Ditto Auto Merge、Paste Pinboards 已支持）
  * 严禁声称“首创双击 `Ctrl+C` 触发”（ClipboardFusion 与经典划词工具已支持）
  * 严禁声称“首创剪贴板离线 OCR 或 MCP 服务”（PowerToys、Raycast 及现有开源 MCP 已支持）
* **✅ 核心对外传播锚点（真实且经验证的差异化）**：
  1. **原生 `Cmd/Ctrl+V` 上的持有式延迟供给架构**：不记新热键、主链路零权限、零键盘钩子、Word 得排版而 Terminal 得源码。
  2. **填补 macOS 生态「AI 时代办公转换型剪贴板」空白**，并实现 macOS + Windows 双端统一。
  3. **一句话自然语言生成声明式 `PipeRecipe` 滤镜配方**。
  4. **连续收集槽（Stack Mode）+ L3 AI 一键结构化汇总**。

## 10. 版本路线图与 MVP 严格边界 (Roadmap & MVP Scope)

### 10.1 Phase 1: MVP v0.1.0（macOS 核心引爆版，研发周期 3–4 周）

**平台范围：仅限 macOS（`13.0+`）**。集中全部研发资源攻克延迟供给主链路与办公富文本保真度，产出极具传播冲击力的 15 秒实机演示闭环。

| 优先级 | 模块名称 | 交付范围与验收标准 |
|---|---|---|
| **[Must]** | **持有式延迟供给核心底座** | 完成 `ClipboardOwner`、`FormatPreserver`（含 Legacy 正则过滤与护栏 G1–G5）、`Materializer`（退出前同步物化 + 所有权校验）、`CallbackBudget`（8ms 超时 + FFI `catch_unwind`）及 `LoopGuard` 闭环，通过全套单元测试。 |
| **[Must]** | **SourceApp 事件流解析与隐私红线** | 基于 `didActivateApplication` 环形缓冲精确解析复制源；内置不可关闭的密码管理器黑名单、`ConcealedType` 过滤与 DLP `SANITIZED_FORCED` 三通道强制脱敏（滤镜 6 基础版）。 |
| **[Must]** | **2 个旗舰 L1 自动挡滤镜** | ① **【AI 星号净化器】**（Markdown → 带 `mso-` 样式的 HTML/RTF 双通道，`plainTextPolicy = PASSTHROUGH`；配套独立 `MANUAL_ONLY` 微信纯文本配方）；② **【社媒/电商链接瘦身】**（淘宝/小红书/B站/京东口令与 UTM 清洗，`plainTextPolicy = CLEANED`）。 |
| **[Must]** | **macOS 兼容性测试矩阵（CI 化）** | 完成 Word for Mac、WPS、飞书、微信桌面版、Pages、VSCode、Terminal、Finder（多文件复制）、进程退出后粘贴共 9 项回归验证，并随 Release Notes 公开发布。 |
| **[Must]** | **Micro-HUD 手动挡（Tier 0 完整闭环）** | 实现零权限 `⌘C⌘C` 差分双击检测与 `⌃⌥V` 热键呼出；常驻隐藏 `WebviewWindow` + 无焦点 `NSPanel`；鼠标偏移定位；260ms 消抖预告 Toast；支持「粘贴原始内容」（DLP 命中时置灰）。 |
| **[Should]** | **Micro-HUD 进阶能力（Tier 1 Opt-in）** | 提供显式开关引导开启 Accessibility 权限，解锁 `AXUIElement` 精确光标跟随与 `enigo` 单次 `⌘V` 自动上屏。 |
| **[Must]** | **历史与贪吃蛇侧边抽屉** | 实现 SQLite FTS5 全文搜索、延迟落盘（`pending` → `ready` 回写）与贪吃蛇连续收集槽（换行拼接、去重编号）。 |
| **[Must]** | **最小闭环 L3 流式引擎** | 接入 OpenAI 兼容流式客户端（预设千问 Flash / GLM-Flash / DeepSeek / 本地 Ollama，支持 BYOK），内置**【贪吃蛇智能汇总】**与**【得体打工人滤镜】**，采用 HUD 内流式预览 + 原子粘贴上屏。 |
| **[Must]** | **10 秒角色破冰与滤镜卡片面板** | 首次启动零权限弹窗破冰页；滤镜卡片支持 `Auto/Manual/Off` 切换、Before/After 实时预览与单滤镜 `plainTextPolicy` 配置。 |

**MVP v0.1.0 端到端验收判据（7 步必过闭环）：**

1. 在 ChatGPT / DeepSeek 复制一段含 `# 标题`、`**加粗**` 与 Markdown 表格的回复；
2. 切到 **Word for Mac** 按原生 `⌘V` → 直接获得带原生标题样式、加粗与实线边框表格的富文本；
3. 切到 **Terminal / VSCode** 按原生 `⌘V` → 直接获得原始无损 Markdown（`#` 与 `**` 完好如初）；
4. 双击 `⌘C⌘C` 呼出 `Micro-HUD`，选择「转微信精致纯文本」或从抽屉点击「粘贴原始内容」→ 正常取回对应文本；
5. **完全退出 PasteFlow 进程**，再切到 Word 与 Terminal 各按一次 `⌘V` → 依然能正常粘出排版富文本与原始 Markdown（验证 §8.8 临终物化生效）；
6. 在 Finder 中选中 **3 个文件** 按 `⌘C` 再按 `⌘V` → 3 个文件完整粘出，未被压平为 1 个（验证 §8.9 护栏 G2/G3 生效）；
7. 全程 **0 个系统权限弹窗**、**0 个必须记忆的新粘贴快捷键**。

### 10.2 Phase 2: v0.2.0（跨平台 Windows 适配与极客生态版）

* **[Must] Windows 平台完整适配**：实现基于 `OleSetClipboard` + 自实现 `IDataObject` 的延迟供给与 `OleFlushClipboard` 临终物化；实现 `WS_EX_NOACTIVATE` HUD 与可见期临时按键转发钩子；完成 Windows 兼容性矩阵。
* **[Must] L2 系统原生视觉引擎上线**：交付【截图秒变 Excel】（框线检测优先 + 坐标聚类兜底 + 可视化微调 UI）与【离线极速 OCR】，内置启动预热与 Windows OCR 语言包缺失探测指引。
* **[Should] 补齐其余 L1 核心滤镜**：上线【网页牛皮癣洗白】、【PDF 断行缝合】与【开发者瑞士军刀】。
* **[Should] AI Pipe Builder 与社区配方仓库**：支持一句话自然语言生成 `PipeRecipe` JSON，支持配方导入导出，同步上线 `awesome-pasteflow-pipes` 官方社区仓库。
* **[Should] Local MCP Server**：一键生成 Cursor / Claude Desktop / Trae 配置文件。
* **[Could] 目标应用分支进阶模式（§3.6）**：先行开展 Windows COM `IServerSecurity::ImpersonateClient` 获取调用方 PID 的技术验证 Spike，根据验证结果决定实现路径。

### 10.3 Phase 3: v0.3.0+（多端协同与高阶管线）

* **[Could] Linux (X11 / Wayland) 平台适配**。
* **[Could] 局域网端到端加密（WebRTC / Local mDNS）跨设备剪贴板空投**（iOS / Android 扫码即传）。
* **[Could] 多步链式管道（Pipe Chaining）**：支持将多个单步滤镜串联组合（如：截图 OCR → 自动翻译 → 转为 Markdown 表格）。

## 11. 分发、许可与可观测性 (Distribution, Licensing & Observability)

### 11.1 代码签名与系统公证

| 平台 | 签名与公证机制 | 预算与发布要求 |
|---|---|---|
| **macOS** | Apple Developer ID Application 证书 + `notarytool` 自动公证（Notarization） | **$99 / 年（v0.1.0 首发前必须完备）**。未公证应用会被 macOS Gatekeeper 直接拦截提示“已损坏或来自身份不明的开发者”，导致大众用户 90% 以上流失。 |
| **Windows** | OV / EV 代码签名证书（或通过 Azure Trusted Signing 签名） | **v0.2.0 Windows 版发布前必须完备**，消除 SmartScreen 蓝屏拦截警告。 |

### 11.2 开源许可与分发渠道

* **开源许可证（已决议 D2）**：核心代码仓库采用 **Apache-2.0 License**（提供明确的专利授权与反诉保护，消除企业内部合规顾虑，每个源码文件头部统一添加 `SPDX-License-Identifier: Apache-2.0`，根目录维护 `NOTICE` 文件）；社区滤镜配方仓库 `awesome-pasteflow-pipes` 采用 **CC0 1.0 Public Domain**，消除配方分享的署名摩擦。
* **核心分发渠道**：
  * **macOS**：GitHub Releases（提供 Universal Binary `.dmg`）+ 官方落地页直链 + **Homebrew Cask**（`brew install --cask pasteflow`）+ 内置 Tauri Updater 自动更新。在未实测验证 App Sandbox 对后台长期持有 `NSPasteboard` 的限制前，不对 Mac App Store 上架作预先承诺。
  * **Windows（v0.2.0）**：GitHub Releases（`.msi` / `.exe`）+ `winget` 官方源 + Microsoft Store。

### 11.3 命名与域名资产保护

* **包管理器占位**：立即在 `crates.io` 与 `npm` 注册并锁定 `pasteflow` 包名（均已核实可用，附录 A.6）。
* **域名与组织**：注册 `pasteflow.dev` 域名并创建 GitHub 组织 `PasteFlowApp`；在域名解析就绪前，`PipeRecipe` 的 `$schema` 统一使用 GitHub Raw 托管地址。

### 11.4 本地优先架构下的可观测性 (Local-First Observability)

由于持有式延迟供给架构中的异常（如护栏主动放弃接管、Provider 回调超时回退、所有权被第三方夺走）在用户体感上均为“静默无感知”，系统必须内置**无需联网的本地诊断面板**：

1. **实时状态与时间轴看板**：展示当前系统剪贴板 `changeCount`、当前持有者状态（`owned` / `materialized` / `lostAt`）、最近 20 次复制事件的 `SourceApp` 解析结果与置信度（`exact` / `stale`）。
2. **护栏与回调健康度指标**：记录每次因护栏 G1–G5 放弃接管的具体编号与源 `types` 清单；统计各滤镜近 7 天触发次数、Provider 回调耗时分位数（p50 / p95 / p99）及超时降级次数。
3. **一键脱敏诊断包导出**：支持导出滚动本地结构化日志（自动剥离一切剪贴板文本内容，仅保留元数据、类型清单、耗时与错误栈），供用户在 GitHub Issues 报障时附带。
4. **Opt-in 匿名遥测（默认关闭）**：严格默认关闭；仅当用户主动开启时上传错误码、OS 版本与耗时分位数，任何情况下绝不采集剪贴板内容。

### 11.5 国际化与双波次首发策略

* **Day-1 双语支持**：UI 界面与 README 文档自 v0.1.0 起同步提供简体中文与英文双语版本。
* **第一波 · 中文社区引爆（少数派、V2EX、即刻、公众号、B站）**：主推**高频痛点场景**——“从 DeepSeek / ChatGPT 复制 Markdown，贴进 Word / 飞书直接是排好版的原生表格与标题”、“淘宝 / 小红书 / B站分享口令一键脱毒瘦身”。
* **第二波 · 全球开发者社区（Hacker News `Show HN`、r/macapps、Product Hunt）**：主推**底层系统架构创新**——“We don't rewrite your clipboard; we own it with lazy data providers. Zero keyboard hooks, zero TCC permissions on the main path, raw Markdown in Terminal and rich formatting in Word from the same `Cmd+V`.”

## 12. 下游 Agent 研发指令指南 (Instructions for Downstream Agents)

当将本文档输入给后续 Agent 推进研发时，**必须严格按以下 6 个阶段顺序串行或受控推进，严禁跳过 Step 0 直接编写业务 UI**：

### Step 0 — 架构地基实测验证 Spike（必须最先执行，阻塞后续阶段）

使用最小化的独立 Rust + `objc2` 程序完成以下 11 项真机实测断言（Swift 原型已验证通过，此处必须在 Rust FFI 层完成闭环验证）：

| 验证编号 | 验证项 | 通过判据 |
|---|---|---|
| **S1** | **Rust `NSPasteboardItemDataProvider` 生命周期与强引用** | Rust 结构体实现协议并安全保活，声明 `html` / `rtf` / `plainText` 三通道，Word 粘贴触发 `html`/`rtf` 回调，Terminal 粘贴触发 `plainText` 回调，跨 FFI 无内存泄漏或悬垂指针。 |
| **S2** | **`plainText` 显式优先级与防 RTF 泄漏** | 验证同时注册三通道时，纯文本消费方 100% 拿到 `plainText` Provider 的返回值，绝不读到 AppKit 从 RTF 自动合成的文本（§8.3 R1–R2）。 |
| **S3** | **生命周期临终物化（退出不丢数据）** | 接管剪贴板 → 触发进程退出（`applicationWillTerminate` / `SIGTERM` / Panic Hook）→ 独立进程读取剪贴板，断言 `types` 非空且能完整读出三通道真实数据（§8.8）。 |
| **S4** | **临终物化的所有权前置校验** | PasteFlow 接管复制 A → 外部进程写入复制 B → 触发 PasteFlow 退出物化钩子，断言 PasteFlow 识别出 `changeCount != changeCountAtOwn` 并放弃物化，B 的内容完好无损。 |
| **S5** | **物化写入与 `LoopGuard` 联动** | 物化操作使 `changeCount` +1 时，断言内部 Watcher 将其识别为自身写入并静默跳过（§8.1）。 |
| **S6** | **源格式直通与 Legacy 正则过滤** | 从 Chrome / Excel / Word 真实复制内容并接管，断言 `org.chromium.source-url` 与 `com.microsoft.excel.data` 等私有格式仍可被原样读出，且无 Legacy UTI 警告（§8.9）。 |
| **S7** | **接管前置护栏 G1–G5 拦截** | 在 Finder 复制 3 个文件（触发 G2/G3）或构造 `>256KB` 大文本（触发 G5），断言 PasteFlow 主动放弃接管，系统原生 `⌘V` 100% 正常粘出 3 个文件。 |
| **S8** | **零权限 `⌘C⌘C` 差分检测与重接管** | 不申请任何系统权限，连续按两次 `⌘C`（间隔 `<250ms`），断言可靠触发 HUD 信号、抑制自动挡 Toast，且第二次复制仍被自动挡成功重接管（§3.5 / §4.1）。 |
| **S9** | **`SourceApp` 窗口事件流抗竞态解析** | 在 Chrome 按下 `⌘C` 后 `50ms` 内极速 `⌘Tab` 切到 Word，断言通过 `didActivateApplication` 环形缓冲记录的 `SourceApp` 依然是 Chrome（§4.1）。 |
| **S10** | **Universal Clipboard (`sharingd`) 交互观察** | 在开启 Handoff 的 Mac 上观察 `sharingd` 拉取 Provider 的时机，验证附加 `ConcealedType` 后能否阻断敏感条目跨设备同步（§8.10）。 |
| **S11** | **常驻隐藏 `micro-hud` 内存基线测量** | 测量 Tauri 2 在 macOS 上挂载常驻隐藏 `micro-hud` `WebviewWindow` 时的核心进程 RSS 与整应用空闲内存，将实测值回填至 §1.3 指标表。 |

### Step 1 — 生成《UI/UX 详细交互与视觉规范 (UI Spec)》

* 基于常驻隐藏 `WebviewWindow`（D1 决议）定义 `Micro-HUD` 的像素级尺寸（宽 `260px`）、毛玻璃明暗色 Token、键盘数字键 `1~4` 高亮态与 L3 流式展开卡片动画。
* 明确区分设计 Tier 0（`已复制，请按 ⌘V` 引导态）与 Tier 1（自动上屏态）的视觉反馈，以及命中 `contains_secret` 时「粘贴原始内容」的锁定置灰态与 260ms 消抖分策略 Toast 样式。

### Step 2 — 生成《Rust 核心引擎技术设计文档 (TDD)》

详细定义以下 10 个核心 Rust 模块的 Trait 接口、线程模型（AppKit 主线程 RunLoop 与 Tokio 异步 Runtime 通信边界）与跨平台条件编译架构：

1. `ClipboardOwner`（延迟供给接管、通道声明、Provider 强引用生命周期、所有权状态机）
2. `FormatPreserver`（源 `types` 枚举、Legacy 正则过滤、护栏 G1–G5 判定、源数据直通缓存）
3. `Materializer`（退出信号监听、所有权前置校验、全通道临终急切物化）
4. `CallbackBudget`（FFI `catch_unwind`、`8ms` 硬超时门控、原始数据回退）
5. `PipeExecutor`（L1/L2/L3 路由、`minConfidence` 门控、`plainTextPolicy` 优先级链解析）
6. `WindowObserver`（`didActivateApplication` 16 槽环形缓冲、`SourceApp` 置信度评估）
7. `CaretLocator`（Tier 1 `AXUIElement`/`UIA` 精确光标获取 + Tier 0 鼠标坐标三级回退）
8. `DoubleCopyDetector`（外部复制差分检测、260ms Toast 消抖取消、二次复制 Re-own 协同）
9. `PrivacyGuard`（密码管理器进程黑名单、`ConcealedType` 检测、`SANITIZED_FORCED` 强制脱敏锁定）
10. `LoopGuard`（`BLAKE3` 环形缓冲、私有 MIME 标记、`expectedChangeCount` 校验）

### Step 3 — 生成《前端状态与 Tauri IPC 接口契约 (IPC Spec)》

* 定义 Rust 后端与三个独立 `WebviewWindow`（`micro-hud` / `drawer` / `main-dashboard`）之间的 `Tauri Commands` 与 `Events` 强类型载荷契约。

### Step 4 — 兼容性测试矩阵工程化

* 构建固定测试语料库（含复杂 Markdown 表格、多级列表、代码块、淘宝/小红书口令、脏 HTML、敏感密钥样本），编写自动化校验脚本与真机回归 Checklist，落地 §8.5 兼容性矩阵。

### Step 5 — 脚手架初始化与 MVP 敏捷交付

* 严格按 `Step 0 Spike → 无 UI 的 Rust 核心底座与 L1 滤镜单元测试 → 挂载 Micro-HUD 与侧边抽屉 → 接入 L3 流式与 10 秒破冰面板` 的顺序推进迭代。

## 附录 A：本机与基准实测数据记录 (Verified Benchmarks)

以下 A.1–A.4、A.6–A.11 均为 **2026-10-14** 在 **macOS 27.0.1 (Build 26A434)** 上的真实代码实测结果；A.5 为 Tauri 2 基准数据。所有架构护栏均直接建立在以下实测证据之上。

### A.1 持有式延迟供给 (`NSPasteboardItemDataProvider`) 基础验证

* **测试方法**：创建 `NSPasteboardItem`，通过 `setDataProvider(_:forTypes:)` 声明 `.string` 与 `.html` 两通道但不写入任何数据；调用 `clearContents()` + `writeObjects()` 后，分别以纯文本消费方与富文本消费方读取。
* **实测输出**：
  ```text
  writeObjects 成功: true
  写入后 provider 调用次数: 0        ← 数据未生成，延迟供给生效
  已声明类型: [public.utf8-plain-text, NSStringPboardType, public.html, Apple HTML pasteboard type]

  [纯文本消费方] 读到: 纯文本通道内容
    → provider 调用记录: ["public.utf8-plain-text"]
  [富文本消费方] 读到: <h2>标题</h2><p>正文<b>加粗</b></p>
    → provider 调用记录: ["public.utf8-plain-text", "public.html"]
  ```
* **实测结论**：写入瞬间零转换计算；消费方仅触发自身请求通道的回调；Provider 对象必须强引用保活。

### A.2 macOS Vision OCR 延迟与冷启动实测

* **测试方法**：程序化生成 900×320 中英混排 4×3 无框线表格位图，调用 `VNRecognizeTextRequest`（`recognitionLanguages = ["zh-Hans", "en-US"]`, `usesLanguageCorrection = true`）。
* **实测数据**：

| 测试场景 | 实测耗时 | 识别文本块数量 | 工程结论 |
|---|---|---|---|
| **机器级首次调用（模型资产物化）** | **13,554 ms** | 10 / 10 | **严禁在粘贴回调内同步运行 OCR**，且应用启动时必须后台预热 |
| **新进程首次调用（系统缓存已热）** | 171 ms | 10 / 10 | 符合预期 |
| **进程内热态 `.accurate` 连续 3 次** | **55 / 52 / 70 ms** | 10 / 10 | 优于 `<100ms` 目标，且中文完全零配置可用 |
| **进程内热态 `.fast` 模式** | 50 ms | **6 / 10（丢 4 块）** | **严禁使用 `.fast` 模式**，会导致表格严重丢单元格 |

### A.3 OCR BoundingBox 表格还原难点实测

* **实测归一化坐标输出（原点左下，y 轴向上）**：
  ```text
  x=0.031 y=0.806 w=0.098 h=0.081 | 项目名称
  x=0.182 y=0.806 w=0.073 h=0.081 | 负责人
  x=0.278 y=0.805 w=0.105 h=0.089 | 预算金额
  x=0.033 y=0.631 w=0.233 h=0.088 | 灵贴 PasteFlow 张三     ← 跨第 1、2 列被合并为单个 observation
  x=0.300 y=0.617 w=0.101 h=0.104 | 120,000                ← 同行数值块，y 值相差 0.014
  x=0.029 y=0.455 w=0.147 h=0.096 | 数据看板重构
  x=0.202 y=0.463 w=0.053 h=0.081 | 李四                    ← 同行 y=0.463
  x=0.293 y=0.448 w=0.087 h=0.110 | 85,500                 ← 同行 y=0.448
  x=0.031 y=0.281 w=0.296 h=0.094 | Quarterly Report Wang Wu ← 跨第 1、2 列合并
  x=0.344 y=0.281 w=0.100 h=0.094 | 240,000
  ```
* **实测结论**：同行 y 坐标存在明显浮动，且相邻列文本极易被合并为单个 Observation。确立 §5.2 **「框线检测优先 + y 重叠容差聚类 + 全局 x 投影二次切分」**策略。

### A.4 Rust 核心依赖库健康度核实（crates.io 基准）

| Crate 名称 | 最新版本 | 最后发布日期 | 累计下载量 | 架构选型裁决 |
|---|---|---|---|---|
| `rdev` | 0.5.3 | 2023-06-26 | 817,753 | ❌ **彻底淘汰**（逾 3 年未维护） |
| `arboard` | 3.6.1 | 2025-08-23 | 53,762,290 | ❌ **不采用**（不支持 HTML / RTF 格式） |
| `clipboard-rs` | 0.3.5 | 2026-06-30 | 743,422 | ✅ **采用并 Vendor**（支持全格式与 Watch） |
| `active-win-pos-rs` | 0.11.0 | 2026-05-26 | 218,697 | ✅ **采用**（仅用于窗口与进程信息，不含 Caret 定位） |
| `enigo` | 0.6.1 | 2025-08-28 | 3,502,092 | ✅ **采用**（用于 Tier 1 粘贴注入） |
| `rusqlite` | 0.40.2 | 2026-08-08 | 124,669,611 | ✅ **采用**（开启 FTS5 全文检索） |

### A.5 Tauri 2 安装包体积与内存基线

* **安装包体积**：Release exe `9.02 MB`，MSI 安装包 `3.00 MB`（满足 `< 10MB` 指标）。
* **内存开销**：Windows WebView2 多进程架构下整应用空闲 Private Memory `183–199 MB`、Working Set `~360 MB`。据此将 §1.3 内存指标拆分为「Rust 核心进程 `< 25MB`」与「整应用内存（待 macOS WKWebView Step 0 实测锁定）」。

### A.6 命名与域名可用性核实

* `pasteflow` 在 `crates.io` 与 `npmjs.com` 均返回 `Not Found`（未被占用，可立即注册）；`pasteflow.dev` 尚未注册。

### A.7 源格式清空破坏面与直通还原实测

* **实测输出**：
  ```text
  接管前：源应用写入 5 个 format → 系统剪贴板实际声明 12 个 type（含 org.chromium.source-url、com.microsoft.excel.data 等）
  调用 clearContents() 后：0 个 type（源私有 UTI 全部被抹除）
  使用 FormatPreserver 直通重建后：12 个 type 全部还原，且 .html 成功覆盖为转换结果
  构造仅含 Legacy 4 字符 Flavor 的剪贴板并强行接管后：types = []，数据完全不可读 ❌
  ```
* **实测结论**：接管必须配合 `FormatPreserver` 直通还原；对纯 Legacy 源必须通过护栏 G1 放弃接管（§8.9）。

### A.8 Provider 进程退出数据蒸发与临终物化实测

* **实测输出（未物化直接退出 Owner 进程）**：
  ```text
  OWNER:  已接管剪贴板, changeCount=31, 声明类型=["public.utf8-plain-text", "NSStringPboardType"]
  OWNER:  进程退出
  READER: changeCount=31, 声明类型=[], 读到内容 = ❌ nil（数据随 Owner 进程消亡而蒸发）
  ```
* **实测输出（退出前执行同步物化）**：
  ```text
  OWNER:  接管后 changeCount=32 → 退出前执行急切物化 changeCount=33 → 进程退出
  READER: 当前 changeCount=33, 读到 plainText = LAZY-PROVIDED-TEXT ✅, 读到 html = <p>LAZY-PROVIDED-HTML</p> ✅
  ```
* **实测结论**：确立 §8.8 生命周期临终物化机制（退出前校验 `changeCount == changeCountAtOwn` 后将全部通道写为真实数据，并登记 `LoopGuard`）。

### A.9 Provider 回调身份盲区与 `⌘C⌘C` 零权限差分检测实测

* **实测结论 ①**：`pasteboard(_:item:provideDataForType:)` 签名仅含 `(pasteboard, item, type)`，不含任何调用方 PID 或 Bundle ID。因此无法在粘贴后区分真实用户粘贴与后台守护进程读取，确立 §4.1 将微反馈 Toast 移至**接管完成后（260ms 消抖）**。
* **实测结论 ②**：对完全相同的选中文本连续按 `⌘C` 时，系统 `changeCount` 严格递增（`28 → 29 → 30`）。确立 §3.5 无需 `CGEventTap` 的零权限 `⌘C⌘C` 检测方案。

### A.10 显式 `plainText` Provider 与 AppKit RTF 自动合成优先级实测

* **实测输出**：
  ```text
  【场景 1】显式注册 plainText + html + rtf 三个 provider：
    纯文本消费方读 public.utf8-plain-text → "RAW-MARKDOWN **bold** # h1" ✅（拿到我方直通原文）
    富文本消费方读 public.rtf             → 含 "TRANSFORMED-RTF" ✅

  【场景 2 · 对照陷阱】只注册 html + rtf，不注册 plainText：
    系统声明 types 中自动冒出 public.utf8-plain-text 与 NSStringPboardType
    纯文本消费方读到 → "TRANSFORMED-RTF" 🔴（AppKit 自动调用 RTF provider 合成了纯文本，转换结果泄漏！）
  ```
* **实测结论**：确立 §8.3 铁律 R1 与 R2——任何时候只要接管了富文本通道，就**必须同时显式注册 `public.utf8-plain-text` 通道**。

### A.11 多 Item 剪贴板压平与 UTI 合法性过滤判据实测

* **实测结论 ①（多文件压平）**：模拟 Finder 复制 2 个文件（`pasteboardItems.count = 2`），用单 Item 接管后 `count` 塌缩为 `1`。确立 §8.9 护栏 G2/G3（多条目或含文件 URL 时放弃接管）。
* **实测结论 ②（`UTType(s) != nil` 判据证伪）**：

| 剪贴板类型标识符 | `UTType(s)` 返回值 | `setDataProvider` 能否注册 |
|---|---|---|
| `public.utf8-plain-text` / `public.html` / `public.rtf` | 非 `nil` | ✅ 可注册 |
| `dyn.ah62d4rv4ge80e2pw`（动态编码 UTI） | 非 `nil` (`isDynamic=true`) | ✅ 可注册 |
| `org.chromium.source-url` / `com.microsoft.excel.data` / `org.nspasteboard.ConcealedType` | ❌ **`nil`** | ✅ **完全可注册** |
| `NSStringPboardType` / `Apple HTML pasteboard type` / `NSFilenamesPboardType` / `CorePasteboardFlavorType 0x...` | ❌ `nil` | ❌ **静默拒绝并打印警告** |

* **结论**：严禁用 `UTType(s) != nil` 过滤直通类型（会静默丢弃 Chrome 与 Excel 的私有格式），必须使用 §8.9 的 Legacy 别名黑名单正则。

## 附录 B：架构与产品已决议项登记表 (Resolved Decisions Register)

本表汇总全部已锁定生效的架构与产品决策，下游研发不得偏离：

| 编号 | 决策议题 | 最终生效决议 | 核心依据与工程约束 | 关联章节 |
|---|---|---|---|---|
| **D1** | `Micro-HUD` 窗口渲染与常驻策略 | **采用常驻隐藏 `WebviewWindow`（不随 30s 销毁）** | 应用启动后延迟 `2s` 在后台预热创建 `micro-hud` 并保持 `visible: false` 常驻，确保 `⌘C⌘C` `0ms` 瞬时弹出；仅 `main-dashboard` 主面板按需创建、关闭后 `30s` 销毁。 | §4.2 / §6.1 |
| **D2** | 开源许可证选择 | **核心代码 `Apache-2.0` + 配方仓库 `CC0`** | Apache-2.0 提供明确专利授权条款，利于企业合规采用；社区配方仓库使用 CC0 消除分享摩擦。 | §11.2 |
| **D3** | 双端代码签名与公证预算 | **macOS 首发前必办（$99/年）；Windows v0.2.0 前必办** | 消除 Gatekeeper 与 SmartScreen 拦截，保障大众层安装转化率。 | §11.1 |
| **D4** | Windows OCR 语言包缺失兜底方案 | **仅探测提示并引导系统安装，不内置离线模型** | 死守 `< 10MB` 安装包体积红线。 | §5.2 |
| **D5** | 目标应用分支进阶模式（§3.6）排期 | **延后至 v0.2.0，先做 Windows COM PID Spike** | macOS 已实测确认 Provider 回调无调用方 PID；v0.1.0 通过 `plainTextPolicy` + HUD 动作解决单通道需求。 | §3.6 / §10.2 |
| **D6** | 包名占位与域名注册 | **立即锁定 `crates.io` / `npm` 的 `pasteflow` 并注册 `pasteflow.dev`** | 域名解析生效前，Schema URL 统一指向 GitHub Raw 地址。 | §7.2 / §11.3 |
| **D7** | 进程退出数据蒸发防护策略 | **采用生命周期临终同步物化 + 首次拉取后内存缓存（取消 1s 定时物化）** | 消除 `t=1s` 定时调用 `clearContents()` 引发的粘贴读取竞态与单次复制 3 次 `changeCount` 跳变；在 `applicationWillTerminate` / 信号钩子 / Panic Hook 中校验 `changeCount == changeCountAtOwn` 后执行同步物化。 | §8.8 / A.8 |
| **D8** | `plainTextPolicy` 覆盖粒度与兜底值 | **全局兜底固定为 `PASSTHROUGH`，仅允许逐滤镜配置，不设全局总开关** | 防止用户一次误设或第三方社区配方静默篡改所有纯文本通道；`SANITIZED_FORCED` 任何层级均不可覆盖。 | §5.4 / §7.2 |
| **D9** | 对外权限口径规范 | **统一表述为“主链路（Tier 0）零权限，精确光标与自动上屏需 1 项辅助功能权限”** | 既有实测支撑（Tier 0 真正 0 权限弹窗），又诚实交代 Tier 1 边界，经得起技术社区源码审视。 | §1.2 / §3.5 / §9.2 |
| **D10** | `⌘C⌘C` 双击检测与自动挡 Toast 协同 | **自动挡 Toast 延迟 `260ms` 弹出；双击触发 HUD 时抑制 Toast 并对第二次复制执行 Re-own** | 彻底消除“第一次复制弹出的 Toast 与第二次复制弹出的 HUD 在同一坐标叠脸撞车”及“按 `Esc` 关闭 HUD 后自动挡失效”两个时序漏洞。 | §3.5 / §4.1 |
