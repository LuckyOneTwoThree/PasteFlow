> **文档状态：** 已归档（Superseded by [[PasteFlow (灵贴) — 产品定义与架构源文档 (Master Spec v1.3)]]）
> **修订日期：** 2026-10-14
> **修订性质：** 架构级修订。核心变更是将「自动挡」的实现路径从 *粘贴时拦截重写* 改为 *复制时持有式延迟供给*，并据实测数据修正了性能指标、权限模型、依赖选型与 MVP 边界。
> **上一版本：** [[PasteFlow (灵贴) — 产品定义与架构源文档 (Master Spec v1.0)]]

---

## 0. v1.0 → v1.1 修订摘要

本节为变更记录，供快速审阅。正文已按此全量修订。

| #      | v1.0 原设定                                                     | v1.1 修订                                                                                                                                                                                                                                                 | 修订依据                                                                          |
| ------ | ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| **A1** | 自动挡 = 拦截 `Ctrl/Cmd+V` → `<1ms` 内取焦点窗口 → **原地重写**剪贴板 → 放行原生粘贴 | 自动挡 = 复制瞬间**持有**剪贴板并声明多通道，消费方来取哪个通道就现算哪个（延迟供给）。**不拦截按键、不改写剪贴板**                                                                                                                                                                                         | 本机实测 `NSPasteboardItemDataProvider` 验证通过（附录 A.1）                              |
| **A2** | 全局键盘钩子是核心链路                                                  | 键盘钩子降级为**可选的进阶能力**（仅目标应用分支需要），非 MVP 依赖                                                                                                                                                                                                                  | 见 B1–B4                                                                       |
| **B1** | 未提及钩子超时风险                                                    | `LowLevelHooksTimeout` 超时后系统不仅放行消息，Win7+ 会**静默卸载钩子且应用无法感知**；微软官方建议钩子线程立即返回、把活丢给后台线程——与"钩子内同步重写后放行"正面冲突                                                                                                                                                  | MS `LowLevelKeyboardProc` 文档                                                  |
| **B2** | 未提及杀软风险                                                      | 正规开发者的 `WH_KEYBOARD_LL` 安装包被 Windows Defender 与赛门铁克标记为 `Trojan:Win32/Fathale.B!plock`                                                                                                                                                                   | Security StackExchange #121948                                                |
| **B3** | "原地重写剪贴板"                                                    | 原地重写是**破坏性**的：从 ChatGPT 复制 markdown、贴进 Word 后剪贴板只剩转换结果，用户再想贴进 IDE 拿原始 markdown 已经没了。v1.0 未识别此漏洞                                                                                                                                                         | 逻辑推演 + DoomPaint 复盘（异步读取方改写导致模态报错，修法为 `OleSetClipboard` + 引用计数 `IDataObject`） |
| **B4** | 未提及权限                                                        | macOS 键盘 `CGEventTap` 需 **Input Monitoring**，检测活动 app / 焦点元素需 **Accessibility**，因此**无法上架 Mac App Store**；社区报告撤销 Accessibility 时若有 active event tap 会导致**系统级输入卡死**（Sequoia/Tahoe 复现）。v1.0 的"第 1 秒零配置"在钩子路线下**不可能兑现**                                     | Karabiner 文档、AeroSpace #1012、Apple Developer Forums                           |
| **C1** | 触发方式含 `Ctrl/Cmd+V+V` 双击                                      | **删除**。第一次 `Ctrl+V` 已把原始内容贴出，要么忍受"贴错再改"的闪烁，要么给每次日常粘贴加 250ms 延迟——两者都违反原则 2。保留 `Ctrl+C+C`（重复复制幂等、零副作用）                                                                                                                                                    | 逻辑推演                                                                          |
| **C2** | macOS 主推 `Option+V`                                          | `Option+V` 在 macOS 系统级映射为 **`√`（平方根）**，要吞掉它必须用 active event tap（= 需 Accessibility，非 listen-only）。macOS 默认触发键改为不冲突组合                                                                                                                                     | 本机验证 + Apple 键盘映射                                                             |
| **C3** | 两端 HUD 行为对称                                                  | **不对称**：`WS_EX_NOACTIVATE` 可不激活窗口但**拿不到键盘焦点**，Windows 上 HUD 收不到 `1~4`/`Enter`，必须从全局钩子转发；macOS non-activating `NSPanel` 可成为 key window                                                                                                                   | MS Q&A #2125382、Raymond Chen 2016-09-12                                       |
| **C4** | `micro-hud` 常驻隐藏、0ms 唤醒；常驻内存 `<25MB`                         | **二者不可兼得**：按需创建 webview 有 100–300ms 冷启动，常驻 webview 则内存下不来。实测 Tauri+WebView2 空闲 private 183–199MB、working set ~360MB。指标重定义见 §1.3                                                                                                                         | 第三方 Tauri/Wails 实测                                                            |
| **D1** | `arboard` / `clipboard-rs` 二选一并列                             | **`arboard` 不支持 HTML/RTF**，无法承载多通道分发；必须用 `clipboard-rs`（支持 Plain/HTML/RTF/Image/FileList/Custom/Watch）。但其下载量仅为 arboard 的 1/70（743k vs 53.7M），**须做好 vendor/fork 准备**；且延迟供给路径两个 crate 均未封装，需自研平台层                                                         | crates.io API 核实 + clipboard-rs 支持矩阵                                          |
| **D2** | `rdev` 用于按键序列监听                                              | **`rdev` 最后发布 2023-06-26，已三年多未更新**。改为平台原生实现（macOS `CGEventTap` / Windows `WH_KEYBOARD_LL`）——反正无焦点窗口与 caret 检测本来就要写原生代码                                                                                                                                  | crates.io API 核实                                                              |
| **D3** | `active-win-pos-rs` 用于获取 `caret_rect`                        | 该 crate 提供窗口位置与进程信息，**不提供 caret 坐标**。caret 需独立实现：macOS `AXUIElement` + `kAXBoundsForRangeParameterizedAttribute`；Windows UIA `TextPattern.GetCaretRange` 或 `GUITHREADINFO.ptCaret`                                                                      | crates.io API 核实                                                              |
| **D4** | Tauri 多 Webview 实例管理                                         | multiwebview 至今仍在 **`unstable` feature flag** 后面。改为多个独立 `WebviewWindow`（稳定 API），或显式接受 unstable 依赖                                                                                                                                                       | Tauri 2.0 Release Notes                                                       |
| **E1** | L2 耗时 `<300ms`                                               | 修正为：**热态 50–100ms（达标）；机器级首次调用实测 13.5s**（模型资产 materialization）。必须做启动预热 + loading 态，否则用户首次截图会以为软件卡死                                                                                                                                                       | 本机实测（附录 A.2）                                                                  |
| **E2** | L2 "系统原生免费 OCR、零安装包膨胀"                                       | macOS 成立（中文零配置可用，实测通过）。**Windows 不成立**：`Windows.Media.Ocr` 依赖 `Language.OCR*` 系统能力包，大量语言为 `State: NotPresent`，`TryCreateFromLanguage` 返回 `null`（PowerToys Text Extractor 即报"没有安装可能的 OCR 语言"）。必须探测 `AvailableRecognizerLanguages` 并优雅降级                  | MS 文档 + PowerToys 文档                                                          |
| **E3** | "配合本地二维坐标聚类算法还原表格行列"                                         | **严重低估**。实测 boundingBox：同一行内 y 值不齐（0.631 vs 0.617），跨列文本被合并成单个 observation（"灵贴 PasteFlow 张三"横跨两列）；`fast` 模式仅识别 6/10 块会丢单元格。改为**框线优先**策略，见 §5 L2                                                                                                          | 本机实测（附录 A.3）                                                                  |
| **E4** | L1 Markdown→Office 视为已解决                                     | Word 忽略大部分 CSS，需 `mso-` 样式或走 RTF 才有保真度；飞书/钉钉/企业微信的 web 编辑器各有粘贴净化器；微信桌面端只吃纯文本。**兼容性测试矩阵升级为一等公民 CI 产物**（详见 §8.5），该矩阵本身即护城河                                                                                                                                | 领域分析                                                                          |
| **F1** | 北极星含 "GitHub 10k+ Stars"                                     | 从北极星**移除**（属传播变量，非产品可控），降为里程碑目标。北极星改为产品指标。同时：西方老牌 OSS 剪贴板工具达 10k+ 用时 8–16 年（Maccy 21,923★/8y、CopyQ 12,461★/16y、Clipy 8,912★/10y），近期唯二快速轨迹为中文生态项目（PasteMD 5,370★/<1y、EcoPaste 7,503★/1.4y）                                                               | api.github.com 核实 2026-10-14                                                  |
| **F2** | "首创三位一体交互"                                                   | **不可声称**。精确化差异化边界，新增 §9 竞品对照与可声称/不可声称清单                                                                                                                                                                                                                 | 竞品扫描（§9）                                                                      |
| **G1** | Phase 1 于 2–3 周交付全部 Must                                     | 该清单实际为 **2–3 个月**工作量，且最耗时的富文本兼容性调优正是 AI agent 最不擅长、最依赖真机反复试的部分。MVP 重新切分见 §10.1                                                                                                                                                                          | 工作量评估                                                                         |
| **G2** | macOS + Windows 同步交付                                         | **macOS 先行**。Windows 侧已有微软官方免费方案 PowerToys Advanced Paste（Win+Shift+V、粘贴为纯文本/Markdown/JSON、**本地 OCR**、AI 动作支持云 key 或本地 Ollama，挂在 139,281★ 的 PowerToys 下）+ PasteMD（Markdown→Word/WPS，自动检测前台应用，5,370★）；macOS 侧该品类为空白（Maccy 只有历史、StopTheMadness 只做 URL 清洗） | 竞品扫描                                                                          |
| **H1** | 无分发/许可/可观测性章节                                                | 新增 §11：代码签名与公证、与其他剪贴板管理器共存策略、License 选择、本地优先下的故障可观测性、i18n 与首发策略                                                                                                                                                                                         | v1.0 缺口                                                                       |
| **H2** | 命名可用性未核实                                                     | 已核实：`pasteflow` 在 **crates.io 与 npm 均未被占用**；`pasteflow.dev` **未注册**——因此 v1.0 中 `PipeRecipe` 的 `$schema` URL 当前为死链，须先注册域名或改用 GitHub 托管的 schema 地址                                                                                                        | crates.io / npm registry / DNS 核实                                             |

---

## 1. 项目元信息与核心定位 (Project Metadata)

### 1.1 基本信息

|属性|定义内容|
|---|---|
|**项目名称**|**PasteFlow**（中文代号：**灵贴**）|
|**一句话 Pitch**|_Don't just copy and paste. Transform in-flight._（懂你正在用什么软件的"自动挡"智能剪贴板）|
|**产品本质**|跨平台操作系统级数据清洗与流转中间件（OS-Level Context-Aware Clipboard Middleware）|
|**目标受众**|**大众层（80%）：** 日常重度使用办公软件、AI 助手、浏览器的白领、运营、学生、财务人员。<br>**极客层（20%）：** 全栈开发者、AI 工程师、效率工具控、开源社区贡献者。|
|**首发平台**|**macOS 优先**（v0.1.0 仅 macOS）。Windows 于 v0.2.0 跟进。依据见 §0 G2 与 §9。|
|**核心差异化**|**持有式延迟供给（Owned Lazy Provision）**：PasteFlow 不改写剪贴板，而是*持有*剪贴板并声明全部格式通道，由各消费方按需拉取对应通道——用户按的仍是原生 `Cmd/Ctrl+V`，而 Word 拿到富文本、微信拿到纯净文本，**零键盘钩子、零权限申请、原始内容无损保留**。|

### 1.2 差异化边界的诚实声明

v1.0 声称"首创三位一体交互"。经竞品核实，该表述**不可用于对外传播**（详见 §9）。可声称的差异化限定为以下四条：

1. **首个以「持有式延迟供给」在原生 `Cmd/Ctrl+V` 上实现零权限、零钩子、无损多格式分发的开源剪贴板中间件。** 现有工具要么在复制时急切改写（CopyQ、ClipboardFusion），要么走备用快捷键（PowerToys Advanced Paste、PasteMD）。
2. **首个跨 macOS + Windows 的开源*转换型*剪贴板。** PasteMD 与 Advanced Paste 仅有 Windows；Maccy 仅有 macOS 且只做历史。
3. **首个「一句话生成声明式滤镜配方」（AI Pipe Builder）。** 社区宏库概念 ClipboardFusion 已有，自然语言生成在此品类中未见。
4. **首个在收集槽消费时做 AI 聚合整理。** 收集槽/合并粘贴本身已存在（Ditto Auto Merge、CopyQ Join Selected Items、Paste Pinboards），AI 聚合层未见。

### 1.3 指标体系（v1.1 重构）

v1.0 将 "10k+ Stars" 与工程指标混列为北极星。v1.1 拆分为三层：

**北极星（产品指标，可控）**

- 日均自动转换触发次数 / 活跃用户 ≥ **20 次**
- **D7 留存 ≥ 25%**
- 社区提交并被合并的 `PipeRecipe` 数量（衡量生态飞轮）

**工程指标（含实测基线，可验证）**

|指标|目标|实测基线 / 备注|
|---|---|---|
|安装包体积|`< 10MB`|✅ **可达**。Tauri 实测 exe 9.02MB / MSI 3.00MB。前提：不内置 OCR 模型|
|Rust 核心进程常驻内存|`< 25MB`|**指标已重定义**：指*空闲且无 resident webview 时*的核心进程 RSS。整应用（含任一 webview）空闲实测 private 183–199MB、working set ~360MB，25MB 对整应用不可达|
|L1 本地转换延迟|`< 5ms`|不变。延迟供给架构下该预算更宽松（仅在消费方拉取时计算）|
|L2 OCR 延迟|**热态 `< 100ms`**|macOS Vision `.accurate` 实测热态 52–70ms。**机器级首次调用实测 13.5s**，须预热 + loading 态|
|首屏可用（无需任何权限）|**0 项权限申请**|✅ 延迟供给路线下达成。仅 caret 定位与"目标应用分支"进阶功能需 Accessibility|

**里程碑目标（传播变量，非北极星）**

- GitHub Stars：v0.1.0 后 3 个月 1k；12 个月 5k（对标 PasteMD/EcoPaste 轨迹）；10k 视为 24 个月以上的长期目标。

---

## 2. 核心设计原则 (Design Principles)

后续任何子文档与代码实现必须严格遵循以下 **6** 条铁律（v1.0 为 5 条，v1.1 新增第 5 条并重排序）：

1. **零权限开箱即用 (Zero-Permission Out-of-the-Box)**
   核心高频功能必须基于本地确定性算法（L1）与系统原生 OCR（L2）实现，用户下载安装后第 1 秒**无需联网、无需注册、无需配置 API Key、无需授予任何系统权限**即可完整体验核心爽感。
   > _v1.1 修订说明：_ v1.0 写作"零配置"，但在键盘钩子路线下该承诺**不可能兑现**（macOS 需 Input Monitoring + Accessibility，且无法上架 MAS）。改为持有式延迟供给后，此原则**首次真正可达成**——这是 v1.1 架构变更最重要的收益。

2. **肌肉记忆零妥协 (Zero Muscle-Memory Friction)**
   绝不破坏用户几十年养成的原生 `Ctrl/Cmd + C` 与 `Ctrl/Cmd + V` 习惯。**任何会给日常粘贴增加可感知延迟的设计一律否决**——据此删除 `Ctrl+V+V` 触发通道（§0 C1）。

3. **无焦点抢占 (Non-Activating UI)**
   任何悬浮提示、光标微胶囊弹出时，绝对不能抢夺当前活动窗口的输入焦点。
   > _v1.1 补充：_ 此原则在两端实现路径**不对称**，不得假设同一套代码（§0 C3）。

4. **本地隐私绝对优先 (100% Local-First & Privacy Guard)**
   所有剪贴板历史仅存储于本地 SQLite；内置不可关闭的密码管理器进程黑名单；支持在发送给外部 AI 前自动执行本地正则脱敏（DLP）。

5. **原始内容无损 (Non-Destructive by Default)** — _v1.1 新增_
   任何转换都不得销毁用户复制的原始内容。原始数据始终由 PasteFlow 进程持有，并可通过 HUD / 抽屉一键"粘贴原始内容"取回。
   > _依据：_ v1.0 的"原地重写剪贴板"违反此原则（§0 B3）。持有式延迟供给在架构上天然满足——我们从不改写，只按需供给。

6. **声明式可扩展 (Declarative Extensibility)**
   所有转换能力抽象为标准化 `PipeRecipe`，支持单文件 JSON/YAML 或自然语言一句话生成，便于社区提交 PR。

---

## 3. 核心架构决策：持有式延迟供给 (Owned Lazy Provision)

> **本节为 v1.1 新增，是整个产品的架构地基。所有下游文档与实现必须以此为准。**

### 3.1 问题：v1.0 的拦截式架构为何不可行

v1.0 设计的链路是：`监听 Ctrl/Cmd+V` → `<1ms 内取焦点窗口` → `匹配 AutoRule` → `原地重写剪贴板多格式缓冲区` → `放行原生粘贴`。

该设计把产品成败压在操作系统最脆弱的一环上，已核实的失败模式：

- **钩子静默失效**：回调超过 `LowLevelHooksTimeout` 后，系统放行消息，且 Win7+ **静默卸载钩子，应用无法感知**。用户会遇到"自动挡莫名不工作了"，而我们收不到任何信号。
- **与官方指引冲突**：微软明确建议低阶钩子应在专用线程上**立即返回**、把工作交给后台线程。而本设计**必须**在钩子内同步完成重写才能保证目标应用读到新内容——两者不可调和。
- **杀软误报**：`WH_KEYBOARD_LL` 安装器被 Defender/赛门铁克标记为木马，对开源项目的安装转化是致命的。
- **破坏性重写**：原始内容丢失（原则 5）。
- **读取竞态**：剪贴板是全局互斥资源。已有公开复盘记录：在异步读取方（如 Paint）读取过程中改写剪贴板，会触发模态错误 `operation did not succeed`；正确解法是 `OleSetClipboard` + 引用计数 `IDataObject`，**而非原地重写**。
- **权限墙**：macOS 需 Input Monitoring + Accessibility，无法上架 MAS，且撤销权限时有 active event tap 会导致系统级输入卡死。

### 3.2 解法：让操作系统的格式协商替我们做"目标应用感知"

**核心洞察：绝大多数场景下我们根本不需要知道 `TargetApp` 是谁。**

在复制发生的那一瞬间，PasteFlow **持有**剪贴板（而非改写它），声明所有可用格式通道；当某个应用来读取时，按它请求的通道**现场计算**并返回对应内容：

- Word / 飞书 / 钉钉文档 / WPS 请求 `public.html`（或 `CF_HTML` / `CF_RTF`） → 得到带原生标题、加粗、表格的富文本
- 微信 / 终端 / VSCode / 记事本 请求 `public.utf8-plain-text`（或 `CF_UNICODETEXT`） → 得到清洗后的纯文本

**"目标软件感知"由操作系统的格式协商自动完成，无需钩子、无需权限、无需在 1ms 内做任何事。**

### 3.3 平台实现

|平台|机制|说明|
|---|---|---|
|**macOS**|`NSPasteboardItem` + `NSPasteboardItemDataProvider`|`setDataProvider(_:forTypes:)` 声明通道；实现 `pasteboard(_:item:provideDataForType:)` 按请求供给。**Provider 对象必须强引用保活**。已在本机实测通过（附录 A.1）|
|**Windows**|`OleSetClipboard` + 自实现 `IDataObject`（延迟渲染 / `DATACACHE`）|这正是公开复盘中针对"改写导致异步读取方模态报错"给出的正确解法。`clipboard-rs` 未封装此路径，**需自研**|

### 3.4 收益清单

采用持有式延迟供给后，一次性消除以下全部风险：

1. 无需全局键盘钩子 → 消除超时静默卸载、杀软误报、`rdev` 依赖
2. 无需 Accessibility / Input Monitoring → **原则 1（零权限）首次可兑现**，且 MAS 上架成为可能
3. 剪贴板从未被重写 → 原始内容无损（原则 5）
4. 不存在与目标应用的读取竞态 → 消除 DoomPaint 类失败模式
5. 转换仅在消费方实际拉取时计算 → `<5ms` 预算宽松，且可按通道做 L1→L2 降级
6. 用户按的仍是原生 `Cmd/Ctrl+V` → **v1.0 追求的"自动挡"体验与叙事完整保留**

### 3.5 残留限制与对策（必须诚实对待）

|限制|说明|对策|
|---|---|---|
|**同一通道只能有一份内容**|"微信要美化纯文本"与"IDE 要原始 markdown"都请求 `plain-text`，无法靠通道区分|① 保守的触发阈值（宁可漏判不可错判）② 按滤镜 / 按目标 app 的开关 ③ HUD 与抽屉提供"粘贴原始内容" ④ 真正的按目标应用分支留作**可选进阶模式**（见下）|
|**macOS 无法识别请求方**|`provideDataForType` 不提供调用方进程信息|接受。macOS 上不做目标应用分支|
|**剪贴板所有权会被夺走**|用户在其他应用复制会调用 `clearContents`，我们的 provider 随之失效|监听 `changeCount`（macOS）/ `WM_CLIPBOARDUPDATE`（Windows），所有权丢失时优雅降级并清理状态（§8.2）|
|**Windows 可能可以识别请求方**|COM 服务端理论上可经 `IServerSecurity::ImpersonateClient` 取得调用方 PID，从而实现真正的目标应用分支|**⚠️ 未验证**，列为 v0.2.0 研究 spike（§10.2）。若成立，Windows 版反而能获得 macOS 拿不到的能力|

### 3.6 可选进阶模式：目标应用分支（Opt-in Auto-Pilot Pro）

对确实需要"同一通道、不同目标、不同内容"的用户，提供**显式开启**的进阶模式，走键盘钩子 + 焦点窗口识别路线。

设计约束：
- 默认**关闭**，开启前必须完整告知权限要求与杀软误报可能性
- 采用**复制时预计算 + 粘贴时 O(1) 查表**：复制瞬间就为各候选目标应用预渲染并缓存（键为 `contentHash × targetApp`），使钩子内只做「取前台窗口 → 哈希查表 → 单次剪贴板写入」，把在钩子内的耗时从 O(转换) 降到 O(1)，尽量贴合超时预算
- 钩子线程必须立即返回，任何 I/O 与转换均在后台线程完成（遵循微软指引）
- 必须实现钩子健康自检与自动重装，并在 UI 中暴露状态（§11.4）——因为系统静默卸载时应用无法感知

---

## 4. 核心交互形态与键盘状态机 (Interaction & Input State Machine)

### 4.1 交互面一：场景感知"自动挡" (Silent Auto-Pilot)

- **触发条件：** 用户在任意应用执行普通的 `Ctrl/Cmd + C`，随后在目标应用执行普通的 `Ctrl/Cmd + V`。**两个按键均不被拦截。**

- **系统行为：**

    1. **复制阶段（Capture & Own）：** 监听到剪贴板变动，Rust 后端读取内容并记录复制源窗口进程信息 `SourceApp`（如 `chrome.exe`、`Cursor`）与内容特征标签 `ContentTags`（如 `is_markdown`、`is_dirty_html`、`is_tracking_url`、`has_secret`）。
    2. **判定与持有（Own）：** 若内容命中任一激活的滤镜且置信度达阈值，PasteFlow **接管剪贴板所有权**，声明全部输出通道（`html` / `rtf` / `plainText`），并将原始内容完整保留在进程内。
    3. **粘贴阶段（Lazy Provision）：** 用户按原生 `Ctrl/Cmd + V`。目标应用向剪贴板请求它偏好的格式，PasteFlow 的 provider 被调用，**此刻才执行对应的 L1/L2 转换**并返回结果。
    4. **视觉微反馈：** 粘贴完成后，在鼠标/文本光标右下角浮现无焦点半透明胶囊 Toast（停留 `800ms` 淡出）：`✨ 已自动净化为 Word 标准排版`。

> _v1.1 修订说明：_ v1.0 的步骤 2「粘贴阶段（Intercept & Morph）」要求在 `<1ms` 内取焦点窗口并原地重写。v1.1 已将该逻辑整体移至复制阶段，粘贴阶段不再有任何拦截行为。

### 4.2 交互面二：光标微胶囊"手动挡" (Cursor Micro-HUD)

**触发通道（v1.1 修订）**

|通道|按键|状态|说明|
|---|---|---|---|
|A（源端双击复制）|按住 `Ctrl/Cmd` 连续双击 `C`（间隔 `< 250ms`）|✅ **默认主推**|重复复制幂等、零副作用，借鉴经典划词工具直觉|
|B（目标端呼出）|macOS：`⌃⌥V`（Control+Option+V）<br>Windows：`Alt+Shift+V`|✅ 备选|见下方冲突说明|
|~~C~~|~~`Ctrl/Cmd+V+V` 双击~~|❌ **已删除**|第一次 `Ctrl+V` 已贴出原始内容。要么忍受"贴错再改"的视觉闪烁与不可靠的撤销，要么给**每一次**日常粘贴加 250ms 延迟——两者均违反原则 2|

> **按键冲突依据（v1.1 新增）：**
> - macOS `Option+V` 系统级映射为 `√`（平方根）。v1.0 将其设为"默认主推"，实际使用时会向文档插入 `√` 字符；要吞掉该按键必须使用 active event tap，而这需要 **Accessibility** 权限（listen-only 的 Input Monitoring 不够）。故 macOS 改用 `⌃⌥V`。
> - Windows 上 `Alt` 单独按下会在 Office 中激活功能区 KeyTips；`Alt+V` 在部分应用中有既有绑定。故 Windows 改用 `Alt+Shift+V`（与抽屉快捷键统一为 `Alt/Option+Shift+V`，见 §4.3，需在实现时消歧或分别指定）。

**UI 形态与行为**

- 紧贴当前文本输入光标（Caret Position，取不到则回退至鼠标指针）弹出毛玻璃微胶囊菜单（宽 `260px`，最多 4 个智能推荐动作）。
- **动态上下文推荐：** 菜单项根据当前 `ContentType` 动态生成（见 §5）。
- **极速上屏：** 按数字键 `1~4`、`Enter` 或鼠标点击后：L1/L2 本地转换瞬间注入目标窗口；L3 AI 流式转换时微胶囊变为呼吸灯态 `⚡ 正在转换...`，以模拟打字机/分块方式输出。
- **新增菜单项：** 始终包含「粘贴原始内容」，兑现原则 5。

> **⚠️ 跨平台不对称（v1.1 新增，必须实现时区分）**
> - **macOS：** `NSPanel` + `NSWindowStyleMaskNonActivatingPanel` 可在**不激活应用**的前提下成为 key window，从而直接接收 `1~4`/`Enter`。
> - **Windows：** `WS_EX_NOACTIVATE` 能使窗口不被激活，但**无法获得键盘焦点**，因此 HUD **收不到**任何键盘消息。数字键与 Enter 必须由全局钩子捕获后转发给 HUD 状态机。
> - 结论：两端的键盘输入路径必须分别实现，不得假设对称。

> **⚠️ HUD 渲染方式与内存指标的冲突（v1.1 新增）**
> v1.0 要求 `micro-hud` "常驻隐藏、0ms 唤醒"，同时要求常驻内存 `< 25MB`。二者不可兼得：
> - 常驻 webview → 实测整应用空闲 private 183–199MB，远超 25MB
> - 按需创建 webview → 100–300ms 冷启动，做不到"0ms 唤醒"
>
> **决策项（需产品确认，二选一）：**
> - **方案甲（推荐，保内存指标）：** HUD 用**原生绘制**，不用 webview。macOS 用 AppKit/CoreText，Windows 用 Direct2D/WinUI。代价：需维护两套原生 UI 代码，动画表现力弱于 Framer Motion。
> - **方案乙（保开发效率）：** 常驻一个隐藏 Tauri `WebviewWindow`，接受整应用空闲 ~60–100MB，并把 §1.3 的内存指标明确改为"Rust 核心进程 < 25MB（不含 webview）"。

### 4.3 交互面三：磁吸式"历史与贪吃蛇"抽屉 (History & Stack Drawer)

- **触发方式：** `Alt/Opt + Shift + V`，或鼠标停靠屏幕边缘。

- **模块 A — 时间轴历史（Timeline）：** 全文毫秒级搜索（SQLite FTS5）、按类型过滤（文本/图片/链接/代码/文件）、常用项置顶（Pin）。
  > _v1.1 补充：_ 每条历史记录**必须同时保存原始内容与各通道转换结果**，以支持"粘贴原始内容"与 Before/After 对比预览。

- **模块 B — 贪吃蛇收集槽（Stack Mode / Basket）：**
  > _v1.1 定位修正：_ 收集槽本身**不是差异化点**——Ditto 的 "Enable Auto Merge"、CopyQ 的 "Join Selected Items"、Paste 的 Pinboards 与多项目同时粘贴均已存在。**真正的差异点在消费时的 AI 聚合层**（下表模式 3），开发与传播资源应向该处倾斜。

    - 抽屉顶部设醒目【🐍 贪吃蛇连续收集】开关（亦可快捷键一键开启）。
    - 开启后，连续 N 次 `Ctrl/Cmd + C` 不互相覆盖，按序压入 `StackQueue[1..N]`，屏幕角落显示微型计数角标 `[🐍 已收集 5 项]`。
    - **一键消费**，三种输出模式：
        1. **换行拼接：** 用换行符或分割线合并所有片段一次性粘贴。
        2. **去重编号：** 剔除重复行并生成 `1. 2. 3.` 有序列表。
        3. **AI 聚合整理（差异化重点）：** 将 N 个碎片作为上下文喂给 L3，直接输出结构化会议纪要、竞品分析表或周报。

### 4.4 交互面四："滤镜商店"主面板与 10 秒破冰 (Main Dashboard & Onboarding)

- **10 秒破冰：** 首次启动不展示复杂设置，仅展示四张角色卡片供多选：
    💼 **日常办公与写作** | 📊 **财务与数据运营** | 🎓 **学生与学术研究** | 💻 **开发者与极客**
  点击【开始使用】后自动激活对应角色的默认自动挡与手动挡滤镜组合。
  > _v1.1 修订：_ 因采用零权限架构，破冰流程**不再需要任何权限引导页**（v1.0 若走钩子路线则必须插入 Accessibility + Input Monitoring 引导，是头号流失点）。仅当用户主动开启 §3.6 进阶模式或 caret 精确定位时，才触发权限引导。

- **可视化卡片管理：** 所有规则包装为"魔法滤镜"卡片，含滤镜名称、适用场景图标、**转换前后对比示例（Before/After Preview）**、启用开关（Auto / Manual / Off）。
  > _v1.1 补充：_ 卡片需支持**按目标应用细粒度开关**，以缓解 §3.5 的"单通道冲突"限制。

- **自然语言造滤镜（AI Pipe Builder，v0.2.0）：** 极简输入框——"用一句话描述你想怎么处理剪贴板内容"（如"把复制的杂乱地址提取出姓名、电话、省市区，并用 Tab 分隔"），自动生成标准 `PipeRecipe` JSON 并挂载到微胶囊。
  > _定位：_ 此为可声称的差异化点之一（§1.2），但排期在 v0.2.0。

---

## 5. 三级引擎能力矩阵 (3-Tier Processing Engine Matrix)

系统严禁将所有任务丢给大模型。所有剪贴板处理按以下三级漏斗路由：

### L1：本地闪电引擎 (Deterministic Local Engine)

|项|内容|
|---|---|
|**定位**|耗时 `< 5ms`；占日常使用 70%；100% 离线、零成本|
|**底层实现**|Rust 原生正则（`regex`）、HTML/Markdown AST 解析（`pulldown-cmark`、`scraper`）、代码结构推断（`serde_json`）|
|**v1.1 执行时机**|在 provider 被调用（消费方拉取某通道）时执行，而非复制瞬间。可选择在复制后异步预渲染并缓存以进一步降低感知延迟|

**核心内置滤镜：**

1. **【AI 星号净化器】** 将 DeepSeek/ChatGPT 复制的 Markdown 转为带原生标题、加粗、表格的 Word/飞书富文本（HTML/RTF 双写）或微信精致纯文本。
   > _⚠️ v1.1 工程警示：_ 此为旗舰滤镜，也是**最难**的一个。Word 会忽略大部分标准 CSS，需要 `mso-` 专有样式或走 RTF 才能获得保真的标题层级与表格边框；飞书/钉钉/企业微信的 web 编辑器各有自己的粘贴净化器会剥离 HTML；微信桌面端只接受纯文本。**必须建立 §8.5 的兼容性测试矩阵并纳入 CI**，不得凭直觉认为"生成了 HTML 就等于 Word 里好看"。
2. **【网页牛皮癣洗白】** 剥离网页复制的刺眼背景色、字号、内联样式，仅保留语义化段落、加粗与列表。
3. **【PDF 断行缝合】** 自动识别中英文 PDF 复制产生的硬回车断行与连字符，合并为通顺自然段。
4. **【社媒链接瘦身】** 剥离淘宝、B站、小红书、京东等分享文本中的追踪参数（`spm=`、`vd_source=` 等）与冗长口令。
   > _v1.1 定位：_ URL 追踪参数清洗在海外已有成熟方案（StopTheMadness Pro、PlainText），**中文电商/社媒口令清洗才是本地化优势**，应作为中文社区首发的传播点。
5. **【开发者瑞士军刀】** JSON 格式化与转 TS/Go/Python/Rust 类型定义；cURL 转 `fetch`/代码；时间戳与 JWT/Base64 实时解码预览。
6. **【DLP 隐私脱敏盾】** 自动识别 API Key（`sk-...`）、数据库连接串、私钥、手机号、身份证号，替换为 `<REDACTED>`。

### L2：系统原生视觉引擎 (Native OS Vision Engine)

|项|内容|
|---|---|
|**定位**|**热态** `< 100ms`（v1.0 为 `< 300ms`）；占日常使用 20%；100% 离线|
|**底层实现**|macOS：`Vision Framework`（`VNRecognizeTextRequest`）<br>Windows：`Windows.Media.Ocr`|
|**排期**|**v0.2.0**（v1.0 曾列入 Phase 1 Must，v1.1 移出，理由见 §10.1）|

**v1.1 实测修正（本机 macOS 27.0.1，详见附录 A.2 / A.3）：**

- ✅ **中文零配置可用**：`recognitionLanguages = ["zh-Hans", "en-US"]` 直接识别成功，无需安装任何语言包。macOS 侧"零配置"承诺成立。
- ⚠️ **必须使用 `.accurate`**：`.fast` 模式耗时相近（50ms）但**仅识别出 6/10 个文本块，会丢单元格**，不可用于表格还原。
- ⚠️ **首次调用极慢**：机器级首次调用实测 **13,554ms**（模型资产 materialization）；新进程首次 171ms；热态 52–70ms。
  → **强制要求：** 应用启动时后台预热 OCR 引擎；预热完成前触发 OCR 必须显示明确 loading 态与预计耗时，否则用户首次截图会判定软件卡死。
- ❌ **Windows 侧"零配置"不成立**：`Windows.Media.Ocr` 依赖 `Language.OCR*` 系统能力包，大量语言处于 `State: NotPresent`，`OcrEngine.TryCreateFromLanguage` 会返回 `null`（PowerToys Text Extractor 即会报"没有安装可能的 OCR 语言"）。
  → **强制要求：** 启动时探测 `AvailableRecognizerLanguages`；目标语言缺失时优雅降级（提示用户安装语言包并给出指引，或提供可选的本地模型兜底——注意这会突破 10MB 安装包指标，须权衡）。

**核心内置滤镜：**

1. **【截图秒变 Excel】** 识别图片中文本块与坐标边界框，按行列对齐还原为 TSV 或 Markdown Table，贴入 Excel 自动分列。
   > _⚠️ v1.1 重大修正：_ v1.0 将其描述为"配合本地二维坐标聚类算法还原表格行列"，**严重低估难度**。实测证据：
   > - 同一行内各 observation 的 y 值不齐（`0.631` vs `0.617`；`0.455` / `0.463` / `0.448`），朴素的 y 相等分组必然失败，需 **y 重叠容差聚类**。
   > - 跨列文本会被合并为**单个** observation：实测中"灵贴 PasteFlow 张三"横跨 `x=0.033, w=0.233`，覆盖了第 1、2 列的边界，按 x 聚类会错误归列；而该行的数值 `120,000` 是**另一个独立** observation（`x=0.300, y=0.617`）。
   > - 无边框表格的列边界必须从**所有行**的 x 间隙做全局投影推断，而非逐行判断。
   >
   > **强制实现策略（框线优先）：**
   > ① 先做**表格框线检测**（macOS `VNDetectContoursRequest` / 形态学横竖线提取）获取精确单元格边界——真实 Excel 截图、网页表格通常有线，此路径可靠性远高于聚类；
   > ② 仅在检测不到框线时，退化到坐标聚类（y 重叠容差分行 + 全局 x 间隙投影分列 + 合并块按列边界二次切分）；
   > ③ 该滤镜的准确率将成为用户投诉的主要来源，需在 v0.2.0 中配套可视化校正 UI。
2. **【离线极速 OCR】** 截图或扫描件直接提取为纯净可编辑文本。
3. **【二维码与色板提取】** 解析图片内二维码链接及主视觉 Hex/Tailwind 色值。

### L3：流式 AI 与 Agent 引擎 (Streaming AI & MCP Engine)

|项|内容|
|---|---|
|**定位**|首字耗时 `< 600ms`；占日常使用 10%；支持 BYOK / 免费 Flash 模型 / 本地 Ollama|
|**底层实现**|OpenAI 兼容流式客户端（预设千问 Flash、智谱 GLM-Flash、Gemini Flash、DeepSeek 及本地 Ollama）；内置 MCP Server（v0.2.0）|
|**v1.1 排期调整**|MVP 仅保留 2 个滤镜（贪吃蛇 AI 汇总、职场高情商润色）；MCP Server 移至 v0.2.0|

**核心内置滤镜：**

1. **【得体打工人滤镜】** 将大白话或情绪化草稿润色为专业、高情商的职场邮件或即时通讯回复。（MVP 保留）
2. **【贪吃蛇智能汇总】** 将收集槽中多条零散记录聚合提炼为结构化摘要或待办清单。（MVP 保留；**此为可声称差异化点**，§1.2）
3. **【跨语境翻译】** 外文段落译为地道母语，或中文转地道英文 Commit Message。
4. **【UI 截图转代码 / 公式识别】** 调用多模态视觉模型还原 LaTeX 公式或前端 Tailwind 组件。
5. **【MCP 上下文桥接】** 作为本地 MCP Server 向 Cursor、Trae、Claude Desktop 暴露脱敏后的剪贴板历史与收集槽数据。
   > _v1.1 定位修正：_ MCP 剪贴板服务器**已有多个既存实现**，作为品类不具差异化。保留功能但降低宣传权重。

> **⚠️ v1.1 新增风险提示（免费额度）：** 预设的千问 Flash / GLM-Flash 免费额度政策随时可能变更。BYOK 方向正确，但让大众层白领去申请 API Key 是一道陡坡。建议：① 首发时明确标注"AI 功能需自备 Key 或使用本地 Ollama"，不承诺免费额度；② 确保所有 L1/L2 能力完全不依赖 L3，使产品在无 Key 状态下依然完整可用（这也是原则 1 的要求）。

---

## 6. 技术栈与系统架构规范 (Technical Architecture & Stack)

Coding Agent 在初始化项目与编写架构设计时，必须采用以下技术选型（v1.1 已按依赖健康度核实结果修订）：

### 6.1 跨端桌面框架

**Tauri 2.0**（严禁使用 Electron，确保极致轻量）。

> _⚠️ v1.1 修正：_ v1.0 要求"多 Webview 实例管理（`main-dashboard` / `micro-hud` / `drawer`）"。Tauri 的 **multiwebview 至今仍在 `unstable` feature flag 后面**（见 Tauri 2.0 Release Notes）。
> **决策：** 改用**多个独立 `WebviewWindow`**（稳定 API）实现三块 UI；若确需单窗口多 webview，必须显式接受 unstable 依赖并锁定版本。

### 6.2 后端核心层（Rust）

|能力|v1.0 选型|v1.1 修订选型|依据|
|---|---|---|---|
|剪贴板多格式读写|`arboard` / `clipboard-rs`|**`clipboard-rs` + 自研平台层**|`arboard`（3.6.1，2025-08-23，53.7M 下载）**不支持 HTML/RTF**，无法承载多通道分发。`clipboard-rs`（0.3.5，2026-06-30，743k 下载）支持 Plain/HTML/RTF/Image/FileList/Custom/Watch on Win+macOS+X11。**下载量仅为 arboard 的 1/70，须做好 vendor/fork 准备**。另：§3.3 的延迟供给路径两个 crate 均未封装，需基于 `objc2` / `windows-rs` 自研|
|**剪贴板持有与延迟供给**|_(v1.0 无此项)_|**新增：macOS `NSPasteboardItemDataProvider`（`objc2`）；Windows `OleSetClipboard` + 自实现 `IDataObject`（`windows-rs`）**|v1.1 核心架构（§3）。已在本机实测 macOS 路径通过|
|全局快捷键|`tauri-plugin-global-shortcut`|不变|—|
|按键序列监听|`rdev`|**移除 `rdev`**，改平台原生（macOS `CGEventTap` / Windows `WH_KEYBOARD_LL`）|**`rdev` 最后发布 2023-06-26，已三年多未更新**。且无焦点窗口与 caret 检测本来就需要原生代码，统一自研更可控|
|按键/粘贴注入|`enigo`|不变（0.6.1，2025-08-28，3.5M 下载，维护良好）|仅 §3.6 进阶模式与 HUD 手动上屏需要|
|活动窗口与进程感知|`active-win-pos-rs` + 原生 API|`active-win-pos-rs`（0.11.0，2026-05-26）**仅用于窗口位置与进程信息**|**该 crate 不提供 `caret_rect`**，v1.0 将其列于"输入光标感知"下为误导|
|**输入光标（caret）坐标**|_(归入上一行)_|**独立实现**：macOS `AXUIElement` + `kAXFocusedUIElement` → `kAXSelectedTextRange` → `kAXBoundsForRangeParameterizedAttribute`；Windows UIA `TextPattern.GetCaretRange` 或 `GUITHREADINFO.ptCaret`|需 Accessibility（macOS）/ UIA（Windows）。**多数应用不支持，必须实现 §8.4 三级回退**|
|无焦点窗口控制|macOS `NSPanel` + `NSWindowStyleMaskNonActivatingPanel`；Windows `WS_EX_NOACTIVATE \| WS_EX_TOOLWINDOW`|不变，但**必须分别处理键盘输入路径**|§4.2：Windows 侧 NOACTIVATE 窗口拿不到键盘焦点|
|本地存储与全文检索|`rusqlite`（SQLite3 + FTS5）+ 缩略图缓存|不变（0.40.2，2026-08-08，124M 下载，维护良好）|—|

### 6.3 前端视图层（TypeScript + React 19）

- 构建与样式：`Vite` + `Tailwind CSS` + `shadcn/ui` + `Framer Motion`（微胶囊弹出动画）
- 窗口架构：`main-dashboard`（按需创建/销毁以省内存）、`micro-hud`、`drawer` —— **以独立 `WebviewWindow` 实现**
- > _⚠️ 若 §4.2 决策采用"方案甲（HUD 原生绘制）"，则 `micro-hud` 不在前端范围内，Framer Motion 仅用于 dashboard 与 drawer。_

### 6.4 系统数据流转图 (Data Flow Pipeline) — v1.1 重绘

```
[System Clipboard Event: 用户执行 Cmd/Ctrl+C]
       │
       ▼
[Rust Watcher & Ownership Guard]
       │  ├─ changeCount / WM_CLIPBOARDUPDATE 去抖
       │  └─(若为自身持有期内的供给回读)──► [Ignore]
       ▼
[Security & Process Filter]
       │  └─(SourceApp ∈ Blacklist: 1Password/Bitwarden/KeePass/Keychain
       │      或含 ConcealedType / ExcludeClipboardContentFromMonitorProcessing)──► [Drop，不进内存不落盘]
       ▼
[Content Analyzer & Tagger] ──► MIME types, SourceApp, ContentTags(JSON/MD/URL/Table/Secret)
       │
       ├──► [SQLite FTS5 Storage]  ← 同时保存「原始内容」与「各通道转换结果」(原则 5)
       │
       └──► [Filter Matcher & Confidence Gate]
               │  (未命中 / 置信度不足)──► [不接管，保持系统原生剪贴板]
               ▼
        [★ OWN THE PASTEBOARD ★]  ← v1.1 核心变更
        声明通道: public.html / public.rtf / public.utf8-plain-text
        provider 持原始数据，此刻【不】执行任何转换
               │
               │   ......用户随后在任意应用按下原生 Cmd/Ctrl+V（不被拦截）......
               ▼
        [Lazy Provision Callback]  ← 消费方请求某通道时触发
               │
               ├─► 请求 html/rtf ──► [L1 Engine <5ms] ──► 渲染富文本 ──► 返回
               ├─► 请求 plainText ──► [L1 Engine <5ms] ──► 清洗/降级排版 ──► 返回
               ├─► 内容为图片且滤镜需 OCR ──► [L2 Engine 热态 <100ms] ──► 返回
               │
               ▼
        [Mini Toast: ✨ 已自动净化为 ...]（无焦点，800ms 淡出）


[Path B: Micro-HUD 手动挡 —— 独立于上述主链路]
  Ctrl+C+C 双击 / ⌃⌥V (mac) / Alt+Shift+V (win)
       │
       ▼
  [Caret Locator] ──三级回退(§8.4)──► [Non-Activating HUD]
       │   macOS: NSPanel 可成为 key window，直接收 1~4/Enter
       │   Windows: NOACTIVATE 无焦点 ⇒ 键盘事件由全局钩子转发
       ▼
  [User Selects Pipe] ──► L1 / L2 / L3 ──► [enigo 注入目标窗口]
                                          └─► L3 流式: 呼吸灯 + 分块打字机输出


[Path C: 目标应用分支 —— §3.6 可选进阶模式，默认关闭，v0.2.0+]
  复制时预渲染各候选 TargetApp 结果并缓存 (contentHash × targetApp)
       ▼
  全局钩子捕获 Ctrl+V ──► 取前台窗口 ──► O(1) 哈希查表 ──► 单次剪贴板写入 ──► 立即返回
       │                                                        (钩子线程禁止任何 I/O)
       └──► [Hook Health Monitor] 自检 + 自动重装 + UI 状态暴露（系统静默卸载时应用无法感知）
```

---

## 7. 核心数据结构与协议定义 (Core Data Schemas)

### 7.1 剪贴板数据条目 (`ClipboardItem`) — v1.1 修订

```typescript
export interface ClipboardItem {
  id: string;                  // UUID v7 (按时间有序)
  createdAt: number;           // Unix Timestamp (ms)
  contentHash: string;         // BLAKE3 Hash (去重 + 预渲染缓存键)
  primaryType: 'text' | 'html' | 'rtf' | 'image' | 'files';

  /** v1.1 变更：原始内容与各通道产物分离存储，兑现原则 5「原始内容无损」 */
  original: {
    plainText?: string;
    html?: string;
    rtf?: string;
    imagePath?: string;        // 本地缓存路径
    filePaths?: string[];
  };

  /** v1.1 新增：延迟供给的通道产物。key = 通道标识，value = 已渲染内容或待计算标记 */
  channels?: {
    html?:     { status: 'pending' | 'ready' | 'failed'; data?: string; pipeId?: string };
    rtf?:      { status: 'pending' | 'ready' | 'failed'; data?: string; pipeId?: string };
    plainText?:{ status: 'pending' | 'ready' | 'failed'; data?: string; pipeId?: string };
  };

  sourceContext: {
    appName: string;           // e.g., "Google Chrome", "Cursor"
    processName: string;       // e.g., "chrome.exe"
    windowTitle?: string;      // e.g., "DeepSeek - 探索未至之境"
  };

  tags: Array<
    | 'markdown' | 'dirty_html' | 'url_with_tracker' | 'pdf_broken_lines'
    | 'json' | 'curl' | 'code' | 'contains_secret' | 'table_image'
  >;

  isPinned: boolean;
  inStack: boolean;            // 是否处于贪吃蛇收集槽中

  /** v1.1 新增：所有权与降级状态（§8.2） */
  ownership: {
    owned: boolean;            // 当前是否仍持有系统剪贴板
    changeCountAtOwn: number;  // 接管时的 changeCount / 序列号，用于检测被夺走
    lostAt?: number;           // 所有权丢失时间戳
  };
}
```

### 7.2 声明式滤镜配方协议 (`PipeRecipe`) — v1.1 修订

```json
{
  "$schema": "https://raw.githubusercontent.com/PasteFlowApp/pasteflow/main/schema/pipe-v1.json",
  "id": "builtin.markdown-to-office",
  "name": "AI 星号净化器 (Markdown 转办公格式)",
  "description": "将 AI 助手复制的带 # 和 ** 的文本，自动转换为 Word/飞书原生加粗与标准表格",
  "author": "PasteFlow Official",
  "version": "1.1.0",
  "icon": "Sparkles",
  "category": "office",
  "tier": "L1_LOCAL",

  "trigger": {
    "mode": "AUTO_AND_MANUAL",
    "matchContentTags": ["markdown"],
    "matchSourceApps": ["*"],

    "v1_1_note": "ownChannels 取代 v1.0 的 matchTargetApps。延迟供给架构下不识别目标应用，改为声明本滤镜愿意供给哪些通道，由消费方的格式请求自然完成分发。",
    "ownChannels": ["html", "rtf", "plainText"],

    "matchTargetApps": null,
    "matchTargetApps_note": "仅在 §3.6 可选进阶模式（目标应用分支）下生效；默认架构忽略此字段。",

    "minConfidence": 0.85,
    "minConfidence_note": "v1.1 新增。宁可漏判不可错判，避免给普通文本误注入 HTML 导致 Slack/Discord 等出现意外富文本。"
  },

  "processor": {
    "engine": "builtin_rust_handler",
    "handlerName": "markdown_to_rich_office",
    "fallbackPrompt": null
  },

  "output": {
    "channels": {
      "html":      { "handler": "markdown_to_cf_html", "note": "需含 mso- 样式以保 Word 标题/表格保真度" },
      "rtf":       { "handler": "markdown_to_rtf" },
      "plainText": { "handler": "markdown_to_pretty_text", "note": "微信/终端通道：降级排版，保留可读性" }
    },
    "preserveOriginal": true,
    "preserveOriginal_note": "v1.1 新增，兑现原则 5。HUD 与抽屉须可取回 original。",
    "toastMessage": "✨ 已自动净化为办公文档格式"
  }
}
```

> _⚠️ v1.1 修正：_ v1.0 的 `$schema` 指向 `https://pasteflow.dev/schema/pipe-v1.json`。经核实 **`pasteflow.dev` 尚未注册**，该 URL 当前为死链。**行动项：** ① 注册域名后再启用该地址，或 ② 暂用 GitHub 托管的 schema 地址（如上）。命名可用性已核实：`pasteflow` 在 **crates.io 与 npm 均未被占用**。

### 7.3 MCP Server 暴露接口规范 (`MCP Integration`, v0.2.0)

> _v1.1 定位修正：_ MCP 剪贴板服务器已有多个既存实现，**作为品类不具差异化**（§1.2）。功能保留，排期移至 v0.2.0，宣传权重下调。

开启本地 MCP 服务时，向外部桌面 AI Agent 暴露：

- **Resources:**
    - `clipboard://latest` — 当前最新一条剪贴板内容（默认经 L1 DLP 脱敏）
    - `clipboard://stack` — 当前贪吃蛇收集槽中聚合的所有多模态/文本片段
    - `clipboard://history?limit=10` — 最近 N 条复制记录
- **Tools:**
    - `run_clipboard_pipe(pipe_id, input_text)` — 允许外部 Agent 调用内置转换与清洗管道
    - `push_to_clipboard(content, format)` — 允许外部 Agent 按富文本或特定格式写入系统剪贴板
      > _⚠️ 新增约束：_ 此 Tool 会夺取 PasteFlow 的剪贴板所有权，实现时必须触发 §8.2 的所有权变更处理，并记入审计日志。

---

## 8. 关键技术壁垒与避坑指南 (Critical Edge Cases & Engineering Guardrails)

v1.0 列出 4 项。v1.1 修订其中 3 项、新增 4 项，共 **7** 项。编写 TDD 与实现代码时必须逐项攻克。

### 8.1 防剪贴板死循环（Loop Guard）— v1.1 修订

- **v1.0 描述的问题：** 软件修改剪贴板写回系统时，会再次触发自身的 Watcher。
- **v1.1 问题形态变化：** 采用延迟供给后，主链路**不再改写剪贴板**，因此经典的"写回自触发"大幅减少。但仍需处理：
    1. provider 被调用时若内部再读写剪贴板，可能触发 changeCount 变化 → 需以 `ownership.changeCountAtOwn` 为基准判定
    2. §3.6 进阶模式与 HUD 注入路径仍会真实写入剪贴板 → 保留 v1.0 方案
- **强制方案：**
    - 每次内部写入前计算 `BLAKE3 Hash` 存入内存环形缓冲区 `RecentInternalWrites`，并附带自定义私有格式标识（`application/x-pasteflow-internal`）。Watcher 检测到标识或匹配 Hash 时短路跳过。
    - 同时维护 `expectedChangeCount`，仅处理非预期的变更。

### 8.2 剪贴板所有权丢失处理（Ownership Loss）— v1.1 新增

- **问题：** 持有式架构下，用户在其他应用复制会调用 `clearContents`（macOS）/ 触发 `WM_CLIPBOARDUPDATE`（Windows），我们的 provider 随之失效。若不处理，会出现"以为还在供给、实际已被顶掉"的静默错误。
- **强制方案：**
    - 持续监听 `NSPasteboard.general.changeCount` / `WM_CLIPBOARDUPDATE`，与 `ownership.changeCountAtOwn` 比对。
    - 检测到所有权丢失 → 标记 `ownership.owned = false`、记录 `lostAt`、清理 provider 强引用（避免泄漏）、停止一切供给。
    - **不得尝试夺回剪贴板**（会与其他剪贴板管理器打架，见 §8.6）。

### 8.3 跨软件粘贴格式兼容性（Clipboard MIME Negotiation）— 保留并强化

- **问题：** 往 Word 贴表格和往微信贴文字所需的底层格式截然不同。
- **强制方案：** 声明并供给全部通道——`CF_HTML` / `public.html`、`CF_RTF` / `public.rtf`、降级排版后的 `CF_UNICODETEXT` / `public.utf8-plain-text`。支持富文本的目标（Word/飞书）自动读 HTML/RTF，仅支持纯文本的目标（微信/记事本）自动读美化纯文本。
- **v1.1 强化：** 此机制现已由**架构本身**保证（§3.2 格式协商），不再依赖运行期判断目标应用。但**"生成了 HTML"≠"Word 里好看"**，见 §8.5。

### 8.4 输入光标坐标获取与回退机制（Caret Position Fallback）— 保留

- **问题：** 原生 Win32 应用 / Electron 应用 / 浏览器 Canvas 中，Accessibility API 不一定能拿到 caret 的精确屏幕坐标。
- **强制方案（三级回退）：**
    1. `UIAutomation`（Win）/ `AXUIElement`（Mac）获取精确 caret 坐标
    2. 失败 → 回退至鼠标指针坐标偏移 `(12px, 12px)`
    3. 鼠标在屏幕外 → 回退至活动窗口正中偏下
- **v1.1 补充：** 此能力需 Accessibility（macOS）/ UIA（Windows）权限，**不属于零权限核心链路**，仅在 HUD 手动挡中使用。权限缺失时直接走第 2 级回退，不得阻塞或反复弹窗索要权限。

### 8.5 富文本目标应用兼容性矩阵（Compatibility Matrix）— v1.1 新增，一等公民

- **问题：** L1 旗舰滤镜的价值完全取决于"转换结果在目标应用里是否真的好看"。Word 忽略大部分标准 CSS（需 `mso-` 专有样式或 RTF）；飞书/钉钉/企业微信的 web 编辑器各有粘贴净化器；微信桌面端只吃纯文本；Excel 对 TSV 与 HTML table 的分列行为不同。
- **强制方案：** 建立并维护兼容性测试矩阵，**作为 CI 中的一等公民产物**，而非临时手工验证：

|目标应用|需验证的通道|重点验证项|
|---|---|---|
|Word 2016 / 365（Win & Mac）|HTML, RTF|标题层级、加粗、有序/无序列表、表格边框与合并单元格、代码块等宽字体|
|WPS Office|HTML, RTF|同上（WPS 对 `mso-` 支持度与 Word 不同）|
|飞书 / Lark 文档|HTML|粘贴净化器会剥离哪些样式；表格是否保留|
|钉钉文档 / 企业微信文档|HTML|同上|
|微信桌面版 / WeChat|plainText|只接受纯文本；emoji 与项目符号的可读性|
|Excel / 飞书表格|TSV, HTML table|自动分列是否正确；千分位与日期是否被误解析|
|Notion|HTML, Markdown|Markdown 直通 vs HTML 渲染的优先级|
|VSCode / Terminal / iTerm2|plainText|必须是原始 markdown/代码，**不得**被美化|
|Slack / Discord|plainText|⚠️ 意外注入 HTML 会导致格式错乱（§3.5 单通道冲突的典型受害方）|

- > _战略说明：_ **这个矩阵本身就是护城河。** 竞品无人系统性地维护它，而它恰恰是最耗时、最依赖真机反复试、AI agent 最不擅长的部分。建议每次 release 附上矩阵通过情况。

### 8.6 与其他剪贴板管理器共存（Coexistence）— v1.1 新增

- **问题：** 用户可能同时运行 Ditto / Maccy / CopyQ / EcoPaste / PowerToys / Windows Win+V 历史。两个组件同时争夺剪贴板所有权会导致内容丢失、互相触发死循环、以及"我的转换莫名失效"。
- **强制方案：**
    - 启动时探测常见剪贴板管理器的运行状态（进程名匹配），若检测到则**主动提示用户存在冲突风险**并建议二选一，而非静默竞争。
    - 严格遵守 §8.2：所有权被夺走时**不尝试夺回**。
    - 在 Windows 上写入时附带 `ExcludeClipboardContentFromMonitorProcessing` / `CanIncludeInClipboardHistory = 0`，避免污染 Win+V 历史（对 DLP 标记为敏感的条目**强制**如此）。
    - 提供诊断面板显示当前剪贴板所有者与 changeCount 时间线，便于用户自查（§11.4）。

### 8.7 隐私安全绝对红线（Security Blacklist）— 保留（已核实准确）

- **强制方案：** 内置**不可关闭**的敏感源过滤。当复制源进程为 `1Password`、`Bitwarden`、`KeePass`、`KeePassXC`、`Keychain Access`，或内容携带系统级隐私标记时，**完全忽略该次复制：不进内存、不落盘、不触发任何管道、不接管剪贴板所有权**。
- **v1.1 核实结果：✅ v1.0 此节技术描述准确，机制均真实存在：**
    - macOS：`org.nspasteboard.ConcealedType`（真实约定，KeePassXC 等在用）
    - Windows：`ExcludeClipboardContentFromMonitorProcessing`（MS 官方文档确认：置入任意数据即可阻止所有格式进入剪贴板历史或同步到其他设备）
    - 相关格式另有 `CanIncludeInClipboardHistory`、`CanUploadToCloudClipboard`（序列化 DWORD，0 = 阻止，1 = 显式请求），以及较早的 `Clipboard Viewer Ignore`
    - 均需通过 `RegisterClipboardFormat` 取得标识值
- **v1.1 补充要求：** 敏感条目**不得**进入 L3 AI 链路，即使用户显式选择 AI 滤镜也须二次确认并本地脱敏。

---

## 9. 竞品定位与差异化边界 (Competitive Positioning) — v1.1 新增

> Star 数经 `api.github.com` 于 **2026-10-14** 核实。

### 9.1 竞品对照

|产品|平台|开源|Stars|与本项目的关系|
|---|---|---|---|---|
|**PowerToys Advanced Paste**|Windows|✅（PowerToys 整体）|PowerToys **139,281**|⚠️ **最大威胁**。微软官方免费：`Win+Shift+V` 预览、粘贴为纯文本/Markdown/JSON/文件、**本地 OCR 图转文**、AI 自定义动作（云 Key 或本地 Ollama/Foundry）。几乎覆盖本项目的 Micro-HUD + L2 OCR + L3 AI 转换territory|
|**CopyQ**|Win/mac/Linux|✅|**12,461**（2009 年至今）|Commands 系统可按**内容正则、MIME 格式、源窗口标题**（`application/x-copyq-owner-window-title`）匹配，在剪贴板变化时自动触发；完整 ECMAScript 脚本 API + 插件 + CLI。**做不到：识别粘贴目标 app**（粘贴为模拟按键）；**无内置 OCR**|
|**Maccy**|macOS|✅|**21,923**|仅历史，无转换能力。证明 macOS 侧"转换型"品类为空白|
|**Clipy**|macOS|✅|**8,912**|仅历史 + snippet|
|**EcoPaste**|Win/mac|✅ (Tauri 2)|**7,503**（2024-05 创建，~1.4y）|**最接近的技术模板**（同为 Tauri 2 跨平台）。已知踩坑：复制企业微信文字曾致闪退，其修法为**禁用 HTML 格式**并加格式白名单——直接印证多格式剪贴板读取的脆弱性（§6.2 vendor 建议）|
|**Ditto**|Windows|✅（镜像）|**7,297**|"Enable Auto Merge" 自动追加连续复制 + "Merge Selected Items"。**即贪吃蛇收集槽的既存实现**|
|**PasteMD**|Windows|✅|**5,370**（2025-10 创建，<1y）|⚠️ **直击旗舰功能**：热键将剪贴板 Markdown 经 Pandoc 转**原生 Word/WPS 富文本**、Markdown 表格转 Excel，且**自动检测前台应用**|
|**ClipboardFusion**|Windows|❌ 免费+Pro|—|**最接近 `PipeRecipe`**：Triggers 支持去格式、文本匹配替换、**C# 宏**；条件包含"**单击复制 vs 双击 Ctrl+C**"（即本项目双触发通道已有商用实现）、按应用忽略；有社区预置宏库|
|**Raycast 剪贴板历史**|macOS|❌ 免费+付费|（extensions 仓 7,785）|记录每条来源 app、图片 OCR 文本、AI commands|
|**StopTheMadness Pro / PlainText**|macOS/iOS|❌|—|URL 追踪参数自动清洗（PlainText 覆盖 200+ UTM 参数）。即 L1 滤镜 4 的海外既存方案|
|**Windows 11 `Win+V`**|Windows|系统内置|—|历史 + 固定，无转换|
|**Alfred Powerpack**|macOS|❌ 付费|—|仅历史|
|**macOS Sequoia/Tahoe**|—|系统内置|—|**无原生剪贴板历史**（据现有资料未见；属"未发现"而非已验证的否定结论）|

### 9.2 不可声称 / 可声称清单

**❌ 不可声称（均有强既存实现，其中数项来自微软）**

- "首创剪贴板内容转换" → CopyQ 自 2009 年
- "首创 Markdown 转 Word 富文本" → PasteMD（5,370★）、PowerToys Advanced Paste
- "首创贪吃蛇/收集槽" → Ditto Auto Merge、CopyQ Join Selected Items、Paste Pinboards、ClipRecall、SmartClipboard
- "首创双击 Ctrl+C 触发" → ClipboardFusion Triggers
- "首创离线 OCR / 截图转表格" → PowerToys Advanced Paste、Raycast
- "首创 MCP 剪贴板服务" → 已有多个实现
- "首创 URL 追踪参数清洗" → StopTheMadness Pro、PlainText，以及任意 CopyQ/ClipboardFusion 正则规则

**✅ 可声称（经核实未发现既存实现）**

1. **在用户原生 `Cmd/Ctrl+V` 上，以「持有式延迟供给」实现零权限、零钩子、无损的多格式按需分发。** 既存工具要么复制时急切改写（CopyQ、ClipboardFusion），要么走备用快捷键（Advanced Paste、PasteMD）。
2. **跨 macOS + Windows 的开源*转换型*剪贴板**（既存转换型方案均为单平台）。
3. **一句话自然语言生成声明式滤镜配方**（社区宏库已有，NL 生成未见）。
4. **收集槽消费时的 AI 聚合整理**（收集槽已有，AI 层未见）。

### 9.3 Stars 目标的现实校准

|项目|Stars|创建|达成耗时|
|---|---|---|---|
|PowerToys|139,281|2019|~6y（微软背书）|
|Maccy|21,923|2018|~8y|
|CopyQ|12,461|2009|~16y|
|Clipy|8,912|2015|~10y|
|EcoPaste|7,503|2024-05|~1.4y|
|Ditto（镜像）|7,297|2020|—|
|PasteMD|5,370|2025-10|<1y|

**结论：** 10k+ 可达但罕见。西方老牌 OSS 剪贴板工具达 10k+ 用时 8–16 年；**近期唯二快速轨迹（PasteMD、EcoPaste）均为中文生态项目，主打 Word/WPS/飞书场景**。

**⇒ 对 §11.5 首发策略的直接含义：** 中文办公场景滤镜（淘宝/小红书/B站口令清洗、微信/飞书适配）**是已被验证的增长杠杆，不是本地化负担**。v1.0 审阅中曾建议"首发主推全球通用滤镜、弱化中文场景"——该判断**已被数据推翻并撤回**。

---

## 10. 版本路线图与 MVP 严格边界 (Roadmap & MVP Scope)

### 10.1 Phase 1: MVP v0.1.0（核心引爆版）

> **v1.1 排期修正：** v1.0 要求 2–3 周交付全部 Must，经评估实际为 **2–3 个月**工作量。原因：跨平台键盘钩子、无焦点 HUD、caret 检测、6 个 L1 滤镜、2 个 L2 OCR 滤镜、FTS5 历史、Stack Mode、L3 流式、MCP、破冰引导、滤镜面板——且其中最耗时的**富文本目标应用兼容性调优**（§8.5）恰恰是最依赖真机反复试、最难由 AI agent 完成的部分。
>
> v1.1 将 MVP 重新切分为**真正可在 3–4 周内做出可传播 demo** 的一刀。

**平台：仅 macOS**（Windows 移至 v0.2.0）。依据：① Windows 侧已被 PowerToys Advanced Paste（微软免费）与 PasteMD（5,370★）覆盖，L2/L3 卖点近乎被官方免费方案抵消；② macOS 侧转换型品类为空白（Maccy 仅历史）；③ macOS Vision OCR 实测中文零配置可用，而 Windows OCR 语言包经常缺失；④ 主力开发环境为 macOS，可日常 dogfood。

|优先级|范围|说明|
|---|---|---|
|**[Must]**|**持有式延迟供给主链路**|`NSPasteboardItemDataProvider` 接管剪贴板、声明 html/rtf/plainText 三通道、按需供给、所有权丢失处理（§8.2）。**这是整个项目的地基，须最先跑通并有单元测试**|
|**[Must]**|**2 个杀手级 L1 滤镜**|① AI Markdown → 办公富文本 ② 社媒/电商链接瘦身。其余 4 个 L1 滤镜移至 v0.1.x 补丁|
|**[Must]**|**§8.5 兼容性矩阵（macOS 子集）**|Word for Mac、WPS、飞书、微信、Pages、VSCode、Terminal。**纳入 CI，作为 release 附件**|
|**[Must]**|**侧边抽屉**|SQLite FTS5 历史搜索 + **贪吃蛇收集槽 Stack Mode**（换行拼接 / 去重编号两种消费模式）|
|**[Must]**|**HUD 手动挡**|`Ctrl+C+C` 触发；caret 三级回退（§8.4）；non-activating `NSPanel`；含「粘贴原始内容」项|
|**[Must]**|**L3 接入（最小）**|OpenAI 兼容流式（预设智谱 GLM-Flash / 千问 Flash / DeepSeek / Ollama）+ BYOK；仅 2 个滤镜：贪吃蛇 AI 汇总（差异化点）、职场高情商润色|
|**[Must]**|**10 秒角色破冰 + 滤镜卡片开关**|因零权限架构，**无需任何权限引导页**|
|**[Must]**|**隐私黑名单（§8.7）不可关闭**|ConcealedType 检测 + 进程黑名单|
|**[Should]**|Toast 微反馈、Dark/Light 双主题|影响 demo 观感，尽量纳入|
|**❌ 明确移出 MVP**|~~Windows 适配~~、~~L2 OCR（截图转 Excel / 离线 OCR）~~、~~MCP Server~~、~~自然语言造滤镜~~、~~目标应用分支进阶模式（§3.6）~~、~~Linux~~、~~`Ctrl+V+V` 触发~~（永久删除）|移入 v0.2.0+|

**MVP 的成功判据（v1.1 新增）：** 用户在 ChatGPT/DeepSeek 复制一段带 `#`、`**`、表格的 Markdown，切到 Word for Mac 按原生 `Cmd+V`，**直接得到带原生标题层级、加粗、真实表格的排版**；随后切到 Terminal 按 `Cmd+V` 得到原始 Markdown；再从抽屉里选"粘贴原始内容"仍能取回原文。**全程零权限弹窗、零快捷键学习成本。** 这个 demo 即传播素材本身。

### 10.2 Phase 2: v0.2.0（跨平台与生态破圈版）

- **[Must] Windows 适配**：`OleSetClipboard` + 自实现 `IDataObject` 延迟渲染；`WS_EX_NOACTIVATE` HUD（键盘经全局钩子转发，§4.2）；`Windows.Media.Ocr` 语言包探测与降级。
- **[Must] L2 系统原生 OCR**：截图提取文字 + **截图转 Excel 表格**（**框线优先**策略，§5 L2）；OCR 引擎启动预热与 loading 态。
- **[Should] 目标应用分支进阶模式（§3.6）**：Opt-in、预计算 + O(1) 查表、钩子健康自检与自动重装。
  - **研究 spike（前置）：** 验证 Windows COM 服务端能否经 `IServerSecurity::ImpersonateClient` 取得调用方 PID。**⚠️ 目前未验证**；若成立，Windows 版将获得 macOS 无法实现的按目标应用分发能力，成为独立的差异化点。
- **[Should] AI Pipe Builder**：一句话生成 `PipeRecipe` + `.json` 配方导入/导出，建立 `awesome-pasteflow-pipes` 社区仓库。
- **[Should] Local MCP Server**：一键生成 Cursor / Claude Desktop / Trae 对接配置。
- **[Should] 补齐其余 4 个 L1 滤镜**（网页洗白、PDF 断行缝合、开发者瑞士军刀、DLP 脱敏盾）。

### 10.3 Phase 3: v0.3.0+（跨端与高阶演进）

- **[Could]** Linux（X11 / Wayland）适配。
- **[Could]** 局域网端到端加密（WebRTC / Local mDNS）跨设备剪贴板空投（iOS/Android 扫码即传）。
- **[Could]** 复杂多步链式管道（Pipe Chaining，如：截图 OCR → 自动翻译 → 转 Markdown 表格）。

---

## 11. 分发、许可与可观测性 (Distribution, Licensing & Observability) — v1.1 新增

v1.0 完全缺失本章，而这几项直接决定开源项目的安装转化率与长期可维护性。

### 11.1 代码签名与公证

|平台|要求|成本/影响|
|---|---|---|
|**macOS**|Apple Developer ID 证书 + **公证（notarization）**，否则 Gatekeeper 拦截并显示"无法打开，来自身份不明的开发者"|**$99/年**。对大众层用户是硬门槛，必须做|
|**Windows**|代码签名证书，否则 **SmartScreen** 显示"Windows 已保护你的电脑"，需用户点"更多信息 → 仍要运行"|OV 证书年费数百美元；EV 证书可更快积累信誉但更贵。**对开源项目安装转化影响极大，须在 v0.2.0 前解决**|

> **行动项：** 未签名发布等于把大部分大众层用户挡在门外。预算须提前规划；README 中若暂无签名版本，必须给出清晰的绕过指引与风险提示。

### 11.2 许可与分发渠道

- **License 决策项（待确认）：** 建议 **Apache-2.0**（含专利授权条款，利于企业采用）或 **MIT**（最简、生态最广）。**不建议 GPL 系**——会显著阻碍企业内采用与商业集成，对追求 star 与社区贡献的开源项目不利。
- **Mac App Store：不可行**（若启用 §3.6 进阶模式则需 Accessibility，MAS 应用无法获得该权限）。即便走零权限的 MVP 路线，MAS 的沙盒也会限制剪贴板持有行为。**主分发渠道为 GitHub Releases + 自建下载页 + Homebrew Cask**。
- **自动更新：** Tauri updater（需自建或托管 update manifest）。
- **Windows 分发：** 考虑 Microsoft Store（可规避 SmartScreen）与 `winget` 提交。

### 11.3 命名与域名（已核实）

|资源|状态|行动|
|---|---|---|
|`pasteflow` @ crates.io|✅ **未被占用**|尽快占位发布|
|`pasteflow` @ npm|✅ **未被占用**|尽快占位|
|`pasteflow.dev`|⚠️ **未注册**（DNS 不解析）|注册后启用 §7.2 的 `$schema` 地址；在此之前使用 GitHub 托管 schema|
|GitHub Org|待创建|建议 `PasteFlowApp`（与 §7.2 schema URL 一致）|

### 11.4 可观测性（本地优先前提下）

- **问题：** 原则 4 要求本地优先、不做行为追踪。但 §8.1 指出钩子会被系统**静默卸载且应用无法感知**，§8.2 的所有权丢失同样是静默的。**没有可观测性，我们无法知道自动挡是否还在工作，用户只会觉得"这软件时灵时不灵"。**
- **强制方案：**
    1. **本地诊断面板**（默认提供，无需联网）：显示当前剪贴板所有者、`changeCount` 时间线、钩子健康状态与最近重装记录、各滤镜近 7 天触发次数、provider 调用成功率与耗时分位数。
    2. **本地结构化日志**（滚动、可一键导出），用于用户提 issue 时附带。
    3. **可选的匿名错误上报**：默认**关闭**，首次开启时明确列出将上传的字段清单，且**绝不包含剪贴板内容**（仅错误码、OS 版本、滤镜 id、耗时分位数）。
    4. **§8.5 兼容性矩阵的 CI 结果**公开发布在每次 release notes 中——既是质量保证，也是差异化传播素材。

### 11.5 i18n 与首发策略

- **双语 README（中/英）从 day 1 开始**，UI 同步双语。
- **首发路径（依据 §9.3 数据）：**
    1. **第一波 · 中文社区**（少数派、V2EX、即刻、微信公众号、B站）：主打**中文办公场景**——"从 DeepSeek/ChatGPT 复制，贴进 Word/飞书直接是排好版的富文本"、"淘宝/小红书分享口令一键瘦身"。这是 PasteMD/EcoPaste 已验证的快速起量路径。
    2. **第二波 · 英文社区**（Hacker News Show HN、r/macapps、r/Productivity）：主打**架构新颖性与开源可审计性**——"零权限、零键盘钩子、无损的剪贴板转换，我们不改写剪贴板，我们持有它"。技术叙事对 HN 受众更有效。
- **演示素材：** 一段 15 秒 GIF/视频，展示"ChatGPT 复制 → Word 粘贴得到原生富文本 → Terminal 粘贴得到原始 Markdown"。**这个对比本身就是产品的全部价值主张。**
- **滤镜分组呈现：** 全球通用组（Markdown→Office、链接瘦身、PDF 缝合、JSON 工具、DLP）与中国特供组（电商/社媒口令清洗、微信/飞书适配）。README 按受众语言调整两组的呈现顺序。

---

## 12. 下游 Agent 研发指令指南 (Instructions for Downstream Agents)

将本文档输入后续 Agent 推进研发时，**必须按以下顺序**拆解任务（v1.1 已重排，spike 前置）：

### Step 0 — 架构验证 Spike（**必须最先做，不得跳过**）

在写任何产品代码前，先用最小 Rust + `objc2` 程序验证 v1.1 的地基：

1. 实现 `NSPasteboardItemDataProvider`，接管剪贴板并声明 `html` / `rtf` / `plainText` 三通道
2. 验证 Word for Mac 通过 `Cmd+V` 收到**原生富文本**（标题层级、加粗、表格），同时 Terminal 收到纯文本
3. 验证原始内容可无损取回
4. 验证所有权被其他应用夺走时的降级行为（§8.2）
5. 测量 provider 回调的实际耗时，确认 `<5ms` 预算

> **理由：** 本项目全部价值主张都建立在"持有式延迟供给在真实目标应用中可用"这一假设上。虽然 provider 机制已在本机以 Swift 验证通过（附录 A.1），但**Rust 侧实现 + 真实 Word/飞书的格式协商行为尚未验证**。若此 spike 失败，整个架构需回退到 §3.6 的进阶模式路线，代价极大——因此必须最先打掉这个不确定性。

### Step 1 — 生成《UI/UX 详细交互与视觉规范 (UI Spec)》

重点定义：`Micro-HUD` 的像素级尺寸、毛玻璃明暗色 Token、键盘导航高亮态、贪吃蛇模式的悬浮角标动画。
**必须包含 §4.2 的 HUD 渲染方案决策（原生绘制 vs 常驻 webview）**，该决策直接影响内存指标能否达成。
**必须分别定义 macOS 与 Windows 的键盘输入路径**（§4.2 不对称性）。

### Step 2 — 生成《Rust 核心引擎技术设计文档 (TDD)》

明确以下核心 Module 的 Trait 接口、线程模型（Tokio 异步 Runtime 与系统主线程 UI 事件循环通信）与跨平台条件编译（`#[cfg(target_os = "macos")]` / `#[cfg(target_os = "windows")]`）：

- **`ClipboardOwner`**（v1.1 新增，**取代 v1.0 的 `ClipboardManager`**）：持有式延迟供给、通道声明、provider 生命周期与强引用保活、所有权状态机
- **`PipeExecutor`**：三级引擎路由、置信度门控、按通道降级
- **`WindowObserver`**：活动窗口与进程感知（`active-win-pos-rs`）、caret 定位与三级回退
- **`KeyhookStateMachine`**：**仅服务于 HUD 手动挡与 §3.6 进阶模式**，非核心链路。含钩子健康自检与自动重装
- **`PrivacyGuard`**：§8.7 黑名单与隐私标记检测，不可关闭
- **`LoopGuard`**：§8.1 + §8.2

### Step 3 — 生成《前端状态与 Tauri IPC 接口契约 (IPC Spec)》

明确 Rust 后端与 React 前端间的 `Tauri Commands` 与 `Events` 载荷格式。
**须以多 `WebviewWindow`（稳定 API）为前提设计，不得使用 multiwebview unstable 特性**（§6.1）。

### Step 4 — 兼容性测试矩阵工程化（v1.1 新增）

将 §8.5 矩阵落地为可执行的测试夹具与 CI 流程：固定测试语料（含 Markdown、脏 HTML、PDF 断行、追踪链接、表格截图）× 目标应用清单 × 期望断言。**该矩阵是护城河，须与功能代码同步建设，不得延后。**

### Step 5 — 脚手架初始化与敏捷迭代

严格按 §10.1 的 MVP 顺序：**先跑通 Step 0 spike → 再无 UI 的 Rust 持有/供给与 L1 转换单元测试 → 再挂载 HUD 与抽屉 → 最后接入 L3 与破冰引导**。

---

## 附录 A：实测数据记录 (Verified Benchmarks)

> 以下数据均于 **2026-10-14** 在 **macOS 27.0.1 (Build 26A434)** 上实测得出，非文档推断。第三方数据已标注来源。固化于此以便后续复现与回归对比。

### A.1 持有式延迟供给（`NSPasteboardItemDataProvider`）— ✅ 验证通过

**测试方法：** 创建 `NSPasteboardItem`，通过 `setDataProvider(_:forTypes:)` 声明 `.string` 与 `.html` 两通道但**不写入任何数据**；`clearContents()` + `writeObjects()` 后分别以纯文本消费方与富文本消费方身份读取。

**实测输出：**

```
writeObjects 成功: true
写入后 provider 调用次数: 0        ← 数据未生成，延迟供给生效
已声明类型: [public.utf8-plain-text, NSStringPboardType, public.html, Apple HTML pasteboard type]

[纯文本消费方] 读到: 纯文本通道内容
  → provider 调用记录: ["public.utf8-plain-text"]
[富文本消费方] 读到: <h2>标题</h2><p>正文<b>加粗</b></p>
  → provider 调用记录: ["public.utf8-plain-text", "public.html"]
```

**结论：** ① 写入后 provider 调用 0 次，证明数据确实延迟生成；② 剪贴板在写入时即**声明**了两个通道，消费方能看到可用性；③ 每个消费方**只触发自己所需通道**的回调；④ 全程未发生任何"重写"，原始数据始终由本进程持有。
**⇒ v1.1 §3 架构成立。**

**实现注意：** provider 对象必须**强引用保活**，否则回调无法送达。

**对照组（多格式同时写入）：** 另测 `setString(html, forType: .html)` + `setString(plain, forType: .string)` 急切写入方式，同样成功且两通道可独立读回（声明类型 `[public.html, Apple HTML pasteboard type, public.utf8-plain-text, NSStringPboardType]`）。**该方式实现更简单但不具备延迟供给与非破坏性优势**，可作为 spike 阶段的降级备选。

### A.2 macOS Vision OCR 性能

**测试方法：** 程序化生成 900×320 含中英文混排"表格"位图（4 行 × 3 列，无框线），`VNRecognizeTextRequest`，`recognitionLanguages = ["zh-Hans", "en-US"]`，`usesLanguageCorrection = true`。

|场景|耗时|识别块数|
|---|---|---|
|**机器级首次调用**（模型资产 materialization）|**13,554 ms**|10|
|新进程首次调用（资产已缓存）|171 ms|10|
|热态 `.accurate` ×3|**55 / 52 / 70 ms**|10|
|热态 `.fast`|50 ms|**6**（⚠️ 丢 4 块）|

**结论：**
- ✅ 中文 OCR **零配置可用**，无需安装任何语言包 → macOS 侧"零配置"承诺成立
- ✅ 热态 `< 100ms`，优于 v1.0 的 `< 300ms` 目标
- ⚠️ **必须用 `.accurate`**：`.fast` 耗时相近但丢失单元格，不可用于表格还原
- ⚠️ **机器级首次调用 13.5s** → 必须启动预热 + loading 态，否则用户首次截图会判定软件卡死
- ❌ **Windows 侧不可类推**：`Windows.Media.Ocr` 依赖 `Language.OCR*` 能力包，常为 `NotPresent`（§5 L2）

### A.3 OCR boundingBox 与表格还原难度

**实测归一化坐标（原点左下，y 向上）：**

```
x=0.031 y=0.806 w=0.098 h=0.081 | 项目名称
x=0.182 y=0.806 w=0.073 h=0.081 | 负责人
x=0.278 y=0.805 w=0.105 h=0.089 | 预算金额
x=0.033 y=0.631 w=0.233 h=0.088 | 灵贴 PasteFlow 张三     ← ⚠️ 跨列合并
x=0.300 y=0.617 w=0.101 h=0.104 | 120,000                ← 同行数值，独立块、y 不同
x=0.029 y=0.455 w=0.147 h=0.096 | 数据看板重构
x=0.202 y=0.463 w=0.053 h=0.081 | 李四                    ← y=0.463
x=0.293 y=0.448 w=0.087 h=0.110 | 85,500                 ← y=0.448
x=0.031 y=0.281 w=0.296 h=0.094 | Quarterly Report Wang Wu ← ⚠️ 跨列合并
x=0.344 y=0.281 w=0.100 h=0.094 | 240,000
```

**观察到的三个硬问题：**

1. **同行 y 值不齐**：第 3 行为 `0.455 / 0.463 / 0.448`，第 2 行为 `0.631 / 0.617`。朴素的 y 相等分组必然失败 → 需 **y 重叠容差聚类**。
2. **跨列文本被合并为单个 observation**："灵贴 PasteFlow 张三" 横跨 `x=0.033 → 0.266`，覆盖第 1、2 列边界；同行数值 `120,000` 却是独立块（`x=0.300`）。按 x 聚类会把它错误归入第 1 列 → 需**按全局列边界对合并块做二次切分**。
3. **列边界须全局推断**：单行无法确定列边界（第 4 行 `Quarterly Report Wang Wu` 宽达 `w=0.296`），必须从**所有行**的 x 间隙做投影聚类。

**⇒ v1.1 强制策略：框线优先**（§5 L2 滤镜 1）。本测试图**无框线**，属最坏情况；真实 Excel/网页表格通常有线，走 `VNDetectContoursRequest` / 形态学横竖线提取可获得精确单元格边界，可靠性远高于纯坐标聚类。

### A.4 Rust 依赖健康度（crates.io API 核实，2026-10-14）

|crate|最新版本|最后发布|下载量|v1.1 判定|
|---|---|---|---|---|
|`rdev`|0.5.3|**2023-06-26**|817,753|❌ **移除**。已三年多未更新，不适合作为核心输入链路依赖|
|`arboard`|3.6.1|2025-08-23|53,762,290|⚠️ 维护良好（1Password 维护）但**不支持 HTML/RTF**，无法承载多通道分发|
|`clipboard-rs`|0.3.5|2026-06-30|743,422|✅ **采用**。支持 Plain/HTML/RTF/Image/FileList/Custom/Watch（Win+macOS+X11）。⚠️ 下载量仅 arboard 的 1/70，**须准备 vendor/fork**|
|`active-win-pos-rs`|0.11.0|2026-05-26|218,697|✅ 采用，但**仅限窗口位置与进程信息**；不提供 `caret_rect`|
|`enigo`|0.6.1|2025-08-28|3,502,092|✅ 采用|
|`rusqlite`|0.40.2|2026-08-08|124,669,611|✅ 采用|

### A.5 Tauri 体积与内存（第三方实测数据）

|指标|数值|对 v1.1 指标的影响|
|---|---|---|
|Release exe|9.02 MB|✅ `< 10MB` 安装包目标可达|
|MSI 安装包|3.00 MB|✅ 同上|
|空闲 Private Memory|183–199 MB|❌ v1.0 的 `<25MB` 常驻目标**不可达**|
|空闲 Working Set（含 WebView2 多进程）|~360 MB|❌ 同上|

**结论：** WebView2 的多进程结构本身即有一笔基础开销——网络、GPU、存储、渲染拆分为独立进程后，**一个很小的页面也会拉起一组进程**。因此 §1.3 已将内存指标重定义为"空闲且无 resident webview 时的 Rust 核心进程 RSS"，并在 §4.2 提出 HUD 原生绘制的备选方案。

### A.6 命名可用性核实

|资源|核实方式|结果|
|---|---|---|
|`pasteflow` @ crates.io|`GET /api/v1/crates/pasteflow`|`crate does not exist` → ✅ **可注册**|
|`pasteflow` @ npm|`GET registry.npmjs.org/pasteflow`|`Not found` → ✅ **可注册**|
|`pasteflow.dev`|HTTPS 探测|连接失败（DNS 不解析）→ ⚠️ **未注册，可用**；但 v1.0 的 `$schema` URL 当前为**死链**|

---

## 附录 B：v1.1 待决策项汇总 (Open Decisions)

以下决策点已在正文标注，集中列出以便一次性确认：

| # | 决策项|选项|建议|正文位置|
|---|---|---|---|---|
|**D1**|HUD 渲染方式|甲：原生绘制（保 25MB 指标，需维护两套原生 UI）<br>乙：常驻 webview（保开发效率，整应用 ~60–100MB）|**甲**，若人力紧张则选乙并同步修改 §1.3 指标|§4.2|
|**D2**|License|Apache-2.0 / MIT / GPL 系|**Apache-2.0**（含专利授权，利于企业采用）|§11.2|
|**D3**|代码签名预算|macOS $99/年（必需）+ Windows OV/EV 证书（数百美元/年）|macOS 立即办；Windows 于 v0.2.0 前办|§11.1|
|**D4**|Windows OCR 语言包缺失时的兜底|仅提示用户安装 / 内置本地模型（突破 10MB）|**仅提示 + 指引**，守住体积指标|§5 L2|
|**D5**|目标应用分支进阶模式（§3.6）是否投入|投入（含 Windows PID spike）/ 永久放弃|v0.2.0 再做 spike 决定，MVP 不投入|§3.6 / §10.2|
|**D6**|域名 `pasteflow.dev` 是否注册|注册 / 改用 GitHub 托管 schema|**注册**（同时锁定 crates.io 与 npm 命名）|§7.2 / §11.3|

---

_本文档为 PasteFlow 的唯一架构真源（Single Source of Truth）。任何子文档、TDD、IPC Spec 与代码实现若与本文冲突，以本文为准；若实现过程中发现本文的技术前提不成立，必须先回改本文并记录于 §0 修订摘要，不得静默偏离。_
