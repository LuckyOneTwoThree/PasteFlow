# PasteFlow (灵贴) — 开发工作计划 (Dev Plan v1.0)

> **文档状态：** 生效中（Aligned with Master Spec v1.4, TDD v1.0, IPC Spec v1.0 & UI Spec v1.0）
> **修订日期：** 2026-10-15
> **上游真源：** [[PasteFlow (灵贴) — 产品定义与架构源文档 (Master Spec v1.4)]]
> **关联文档：** [[PasteFlow (灵贴) — Rust 核心引擎技术设计文档 (TDD v1.0)]] · [[PasteFlow (灵贴) — 前端状态与 Tauri IPC 接口契约 (IPC Spec v1.0)]] · [[PasteFlow (灵贴) — UI与交互设计规范 (UI Spec v1.0)]]
> **计划口径：** 一份完整计划、两个里程碑（v0.1.0 内部验证里程碑 + v0.1.5 对外交付里程碑）、四个阶段（P0–P3）。整体排布至 v0.1.5，但保留 D10 决议的 v0.1.0 止损验证点，避免在核心路线（延迟供给 + FrontmostApp 启发式）未经真机闭环前过度投入交互层。

## 0. 计划总览与裁决原则 (Plan Overview)

### 0.1 为什么按 v0.1.5 整体规划、保留 v0.1.0 里程碑

| 裁决 | 内容 |
|---|---|
| **整体规划至 v0.1.5** | TDD / IPC / UI Spec 的主体契约（Micro-HUD、抽屉、SQLite FTS5、L3 流式、三窗口拓扑）均为 v0.1.5 范畴，一次性排布可避免架构返工与 IPC 二次改造。 |
| **保留 v0.1.0 内部里程碑** | 项目最大技术风险是「DataProvider 延迟供给 + `FrontmostApp` 启发式」的真实跨应用行为（如 Slack 收到 `nil` 是否自动回退读 `plainText`）。v0.1.0 是约 2 周可验证的止损点（D10）：若 Step 0B / E2E 发现核心路线硬伤，HUD、抽屉、L3 的投入建立在沙上。 |
| **v0.1.0 形态约束** | 纯菜单栏 + 原生 Toast，零常驻 Webview（D1）；v0.1.5 再挂载 `micro-hud` / `drawer` / `main-dashboard` 三窗口。 |

### 0.2 阶段总览

| 阶段 | 名称 | 核心交付 | 出口标准 | 排期参考 |
|---|---|---|---|---|
| **P0** | 地基冒烟（Step 0B 跨进程 E2E） | 4 应用真实 `⌘V` 协商冒烟 + `NonActivatingHUDPanel` 真实按键吞吐冒烟 | 核心路线真机确认，偏差回写 Master Spec 附录 | 1–2 天 |
| **P1** | 引擎底座（→ v0.1.0 里程碑） | Tauri 2 工程骨架 + 8 大核心模块 + 5 个 L1 算子 + 3 套测试门禁 + 菜单栏控制中心 + 原生 Toast | 🏁 **v0.1.0**：真机回归 checklist 全过、8ms 预算与 55MB 内存红线达标、15 秒演示视频素材就绪 | ~1 周 |
| **P2** | 交互层（Micro-HUD / 抽屉 / 主面板） | `micro-hud` 常驻隐藏 + 历史抽屉 + 贪吃蛇 Stack + `main-dashboard` + 全量 IPC Commands/Events | 交互全链路真机可用，UI Spec 布局与状态机逐条验收 | ~1.5 周 |
| **P3** | L3 引擎与发布收尾 | L3 流式引擎 + 补齐 3 个 L1 滤镜 + 全量 E2E 回归 + 签名公证发布链路 | 🏁 **v0.1.5** 发布（GitHub Releases `.dmg` + 演示材料） | ~1 周 |

### 0.3 全局铁律（任何阶段不得违反）

1. **原则 7 优先**：宁可不接管，不可破坏原生粘贴（Master Spec §2 冲突裁决优先级）。
2. **8ms 回调硬预算** + `catch_unwind` FFI 隔离 + 预编译正则（禁 `NSDataDetector`）。
3. **`plainText` 通道必须显式注册**（铁律 R1）；HTML/RTF 双通道同步抑制（§3.2）。
4. **退出/暂停前同步物化**（D7），严禁裸 POSIX signal handler 调 AppKit。
5. **TOCTOU 原子 CAS**（§8.1）：`clearContents()` 前最后一行校验 `changeCount`。
6. **DLP 静默三不**（D8）：绝不篡改粘贴明文。
7. 性能与内存实测基线（附录 A.12）是验收锚点，实现与回归均需对齐。

---

## 1. P0 — 地基冒烟：Step 0B 跨进程 E2E（1–2 天）

### 1.1 目标

在进入编码前，用最小宿主进程验证两条核心路线的真实跨应用行为，消除 v1.4 §12 Step 0B 定义的两项准入风险。

### 1.2 任务分解

| 编号 | 任务 | 方法 | 验收断言 |
|---|---|---|---|
| **P0-1** | **S0-E2E：4 应用真实粘贴协商冒烟** | 后台运行最小宿主 Provider（可基于 `spike/step0_verify.swift` 演进），依次切至 **Word / 微信 / Terminal / Slack** 按下真实 `⌘V` | ① `FrontmostApp` 实时读取准确；② Word 获得富文本变体 `rich_office`；③ 微信获得 `im_cleaned_text`；④ Terminal 获得 `raw_passthrough`；⑤ **Slack 在 `public.html`/`public.rtf` 同步返回 `nil` 时 100% 自动回退读取 `public.utf8-plain-text`** |
| **P0-2** | **S8-E2E：非激活 NSPanel 真实按键吞吐冒烟** | TextEdit 保持焦点输入状态，弹出 `NonActivatingHUDPanel` 并聚焦内部 `WKWebView`，物理按下 `1` / `Esc` | 前端 JS `keydown` 100% 收到事件，且按键未泄漏进 TextEdit |
| **P0-3** | **偏差裁决与回写** | 若 P0-1 中某应用收到 `nil` 未自动回退，启用 `<pre>` 转义源码兜底（TDD §3.4 备选路径）；记录实测结论 | 偏差写入 Master Spec 附录 A（新增 A.13 实测记录）并作为 P1 的 `PipeExecutor` 实现输入 |

### 1.3 风险与止损

* **风险 R-P0-1**：Slack / Discord 不回退 `plainText` → 已有兜底方案（`<pre>` 转义源码包），不阻塞计划。
* **风险 R-P0-2**：`WKWebView` 在 `NonActivatingHUDPanel` 内收键异常（macOS 版本差异）→ 降级为 HUD 内隐藏 `NSTextField` 承接按键再转发 JS，仍不阻塞 P1，仅影响 P2 排期微调。

### 1.4 P0 冒烟实测结论与架构冲击（2026-10-15，`spike/step0b_e2e.swift`）

> 本节为 P0-1 / P0-3 的实测产出。**核心架构假说被部分证伪**，但补救路径已验证可行。实测原件见 Master Spec 附录 A.13。

#### 1.4.1 实验矩阵与结果

| 实验 | 方法 | 实测结果 | 判定 |
|---|---|---|---|
| **exp1 跨进程延迟供给 + 路由** | serve 挂载 Provider（html/rtf/plainText）+ 50ms 内部抢读 | +60ms 抢读命中 `probe_guard_passthrough` ✅；但 +339/342ms 被第三方嗅探读走全部通道后 `pasteboardFinishedWithDataProvider` 即触发 | Probe Guard 有效；**嗅探威胁实证** |
| **exp2 跨应用通道协商** | 激活 WeChat / Lark / Cursor / Terminal / Notion 逐个读取 | 5 应用 × 3 通道读取**全程零 provider 回调**，读到数据均为嗅探者固化值 | **per-type 首读冻结实证** |
| **exp3 决定性：同 type 重复读取** | serve（noguard）+ `probe-plain` 连续多应用读取 | plainText 仅在挂载后被嗅探者读 1 次（+109ms `probe_guard_passthrough`）；后续 TextEdit/WeChat 前台读取全部命中缓存、**provider 零重入** | **「每次回调按 FrontmostApp 实时分支」不成立** |
| **exp4 拒供是否重入** | `RefusingProvider` 首次回调不写数据 | 拒供后**不再重新回调**，后续真实读取只得 `nil` | **最坏情形：首读冻结含空值，拒供不能反制嗅探** |
| **exp5 Re-own 补挂救活** | 首次供给后自动 `clearContents()+writeObjects()` 挂载下一代 Provider | **G1→G10 每一代均被真实消费方回调**，`frontmost` 随前台精确切换（TextEdit/WeChat），probe 读到 GEN4/GEN9 逐代递增 | ✅ **补挂救活延迟供给，实时分支恢复** |

**附带发现**：① 系统写入会自动补全 `public.utf16-external-plain-text` 别名（印证铁律 R3，别名无需自注册）；② **本机存在 ≥3 个后台剪贴板嗅探者**（Obsidian / TRAE CN / Gemini 在挂载后 60–340ms 内全通道抢读），不是长尾风险而是常态。

#### 1.4.2 架构冲击（待 Master Spec v1.5 裁决修订）

* **D5（FrontmostApp 启发式）修订必要性**：「Provider 回调时 O(1) 实时分支」仅在**首次回调属于真实粘贴**时成立。一旦嗅探者抢先触发首读，通道内容即被固化为其时点分支（且 exp4 证明拒供只会固化空值）。
* **主补救方案（P1 设计输入）**：**Probe-Guard 触发 Re-own 补挂**——当检测到首读命中 `<150ms` 抢读护栏（`elapsed < 150ms && frontmost == SourceApp`）时，等待**全部声明通道供给完成或 300ms 超时**（取先到者）后重新 `clearContents()+writeObjects()` 挂载新一代 Provider，恢复按目标应用实时分支。
* **补挂边界约束（exp4/exp5 推论）**：① 补挂次数 ≤ 3 代，连续嗅探场景降级为静态供给（路线 B）不再补挂，防无限滚代；② 补挂产生的 `changeCount` 递增必须登记 `RecentInternalWrites` 短路，防 `DoubleCopyDetector` 误判为外部双击复制；③ 一次 `⌘V` 消费禁止混合代际数据（故补挂时机必须等待全通道供给完成，否则慢速消费方会拿到 G(n) plainText + G(n+1) html）。

#### 1.4.3 对 P1 / P2 任务的影响

| 任务 | 影响与修订 |
|---|---|
| **P1-7 `PipeExecutor`** | 路由决策树增补「首读后 Re-own 补挂」触发判断；实测 frontmost 分支逻辑正确（exp1 变体判定准确） |
| **P1-8 `ClipboardOwner`** | 新增 Re-own 补挂机制（≤3 代、全通道供给完成或 300ms 后补挂、内部写入短路登记） |
| **P1-10 `DoubleCopyDetector`** | 必须屏蔽补挂造成的 `changeCount` 自增，避免误触发双击还原/HUD |
| **P2-8 IPC** | `servedHistory` 记录需区分「sniffer 抢读固化」与「真实粘贴供给」，诊断看板展示补挂代数 |
| **§8.5 真机回归** | 新增断言：嗅探者抢读后真实 `⌘V` 仍按目标应用正确分支（补挂生效） |

---

## 2. P1 — 引擎底座：交付 v0.1.0 极简菜单栏引爆版（~1 周）

> 范围 = Master Spec §10.1 Must 清单 + D10 的 v0.1.0 里程碑。**零常驻 Webview**（D1）。

### 2.1 任务分解（按依赖顺序）

| 编号 | 任务 | 依据 | 验收断言 |
|---|---|---|---|
| **P1-1** | **工程骨架**：按 TDD §1.1 目录创建 `src-tauri`（8 模块 + `handlers/` + `platform/macos/` + `storage/` + `ipc/`）、`pipes/`、`schema/pipe-v1.json`、`Cargo.toml`（TDD §1.2 依赖规范）、`tauri.conf.json`（菜单栏 App 形态，`LSUIElement`） | TDD §1.1–1.2 | `cargo build` 通过；安装包体积预算 < 10MB（A.5 基线） |
| **P1-2** | **`WindowObserver`**：16 槽 `Arc` 环形缓冲、`resolve_source_app` 抗竞态回溯、`current_frontmost_app` O(1) 读取 | TDD §3.3 | 单元测试断言 500ms stale 阈值；对齐实测 `0.042 µs` 量级（A.12） |
| **P1-3** | **`FormatGate`**：Stage 1 元数据门控（G1A/G1B/G2/G3/G4）+ Stage 2 双层体积分级（`64KB`/`256KB`） | TDD §3.2 | `cargo test --test guardrails` 全绿（含 Word 伴生 TIFF 放行 / 纯截图拦截 / 120KB 轻量降级 / 600KB 拒绝用例） |
| **P1-4** | **`PrivacyAndLoopGuard`**：内部写入短路、密码管理器黑名单、`SECRET_REGEX` 静默三不 | TDD §3.8 | DLP 命中项：不接管、不入 SQLite、不进 L3；明文粘贴 100% 保留 |
| **P1-5** | **`CallbackBudget`**：`catch_unwind` + 8ms 预算审计降级 | TDD §3.5 | `guardrails` 测试断言 Panic 100% 拦截、超时回退 `raw_fallback` 且 `degraded=true` |
| **P1-6** | **L1 算子（5 个）**：`markdown_html`（`mso-` 样式）、`markdown_rtf`、`markdown_im_text`（单遍 UTF-8 字节流）、`social_link_cleaner`（预编译正则 + 域名分级参数剥离）、`dlp_detector` | TDD §4 | `cargo test --test l1_snapshots` 全绿（10 组语料；断言 `mso-border-alt` 存在、微信纯文本无 `**`、开发者链接 `?from=` 100% 保留） |
| **P1-7** | **`PipeExecutor`**：启发式路由决策树（<150ms Probe Guard → HTML/RTF 双通道抑制 → `plainTextPolicy` 四级分支） | TDD §3.4 | 与 P0 冒烟结论一致；Slack HTML+RTF 同步返回 `None`；IM 白名单净化 / 终端保源码分支准确 |
| **P1-8** | **`ClipboardOwner`**：50ms 轮询、`acquire_ownership_cas` TOCTOU 原子接管、`Retained` 强引用保活、按 `PayloadTier` 声明通道（R1 铁律） | TDD §3.1 | 并发新复制场景断言 `ConcurrentModificationDetected`；`FullMultiChannel` 始终显式注册 `public.utf8-plain-text` |
| **P1-9** | **`Materializer`**：`applicationWillTerminate` + GCD 主线程信号源（SIGTERM/SIGINT）+ Panic Hook + 暂停前置物化四步铁律 | TDD §3.6 / D7 | 复制 → 退出进程 → 独立进程仍可读出全部通道（A.8 场景回归） |
| **P1-10** | **`DoubleCopyDetector` + 260ms 消抖**：`<250ms` 同 hash 同 SourceApp 差分检测、Toast 取消协同、v0.1.0 还原原文动作 | TDD §3.7 | 双击 ⌘C⌘C 弹 `↩️ 已还原为原始纯文本`；单次复制 260ms 后 Toast 正常浮现 800ms |
| **P1-11** | **原生菜单栏控制中心 + `NativeToastController`**：UI Spec §2.1 图标 4 态、§2.2 六分组菜单、§2.3 五种 Toast 文案、§2.4 `accessBehavior` 引导卡 | UI Spec §2 | 菜单栏功能逐项可用；Toast 定位与边缘翻转规则正确；全程零 Webview |
| **P1-12** | **两个杀手滤镜 + 静默 DLP 挂载**：`pipes/builtin.markdown-to-office.json`、`pipes/builtin.social-link-cleaner.json` 通过 `pipe-v1.json` Schema 校验 | Master Spec §5.1 / §7.2 | Word/微信/Terminal/Slack 四场景 15 秒演示脚本可录制 |
| **P1-13** | **快捷键登记表**：`hotkeys.rs` 常量表 + 无冲突断言（v0.1.0 激活：`⌘⇧P`、`⌃⌥V`、`⌃⌥D`） | TDD §3.9 | `cargo test --test hotkey_registry` 全绿 |
| **P1-14** | **真机回归 checklist + 演示素材**：执行 Master Spec §8.5 第二层 9 行 checklist；录制 15 秒爆款演示视频 | §8.5 / §10.1 | 9 行断言全过；`phys_footprint < 55MB`（实测基线 47.33MB）；L1 转换 `p95 < 5ms`（实测 3.98ms） |

### 2.2 🏁 v0.1.0 里程碑出口标准（内部验证里程碑）

1. 自动挡主链路在 Word / 微信 / Terminal / Slack 四类目标应用真机行为与 §3.2 路由表 100% 一致。
2. 9 行真机回归 checklist（§8.5）全过，含「宿主退出/暂停后粘贴不丢内容」。
3. 三套测试门禁（`l1_snapshots` / `guardrails` / `hotkey_registry`）全绿并纳入 CI。
4. 内存红线：v0.1.0 整应用 `phys_footprint < 55MB`；8ms 预算无降级告警（或降级均有审计日志）。
5. 15 秒演示视频素材就绪，具备对外验证市场需求的条件。
6. **止损裁决点**：若 P0/P1-14 发现核心路线存在不可兜底硬伤，暂停 P2/P3 投入，先回到 Master Spec 修订架构。

### 2.3 P1 真机装配实施进度（2026-10-15，P1-15a~e）

**已落地（代码 + 164 测试门禁全绿）**：

| 子任务 | 交付物 | 冒烟结果 |
|---|---|---|
| P1-15a objc2 真机 Provider | `paste_provider.rs`：`declare_class!` 实现 `NSPasteboardItemDataProvider`，回调内 `run_with_budget`（R1 catch_unwind + 8ms）+ O(1) frontmost 路由 + Probe Guard + Re-own 代数 | 编译通过；协议方法位于协议 impl 块（类 impl 块会 panic，已修） |
| P1-15b 接管决策流 | `engine.rs::evaluate_takeover`（G1A–G5 + 双层体积分级）+ `tick_takeover` | 5 项单测覆盖（Pass/G1A/Refuse/Light/MultiItem） |
| P1-15c tauri 装配 | `lib.rs`：零 Webview（无任何 Window）+ TrayIcon 六项菜单 + 50ms changeCount 轮询 + `run_on_main_thread` 挂载 + 退出走 Materializer 四步铁律（R4） | 编译零警告；`build.rs` + `tauri-build` + `icons/icon.png` 补齐（P1 骨架遗漏） |
| P1-15d 真机冒烟 | `./target/debug/pasteflow` 真机启动 | ✅ 启动正常；✅ `pbcopy` 后 50ms tick 触发接管（change_count=780）；✅ `pbpaste` 完整读到内容（Provider 供给正确）；✅ Tray 菜单创建无报错；RSS ≈ 87.8MB（**debug 构建**，含 tauri/objc runtime） |
| P1-15e 文档回写 | 本节 | — |

**冒烟观察项（记入待办）**：
1. 「NSPasteboard: synchronous promise fulfillment requested from a background thread」×3 告警：消费者（`pbpaste`）在后台请求 Provider 供给所致（与 P0 exp2 现象一致），非阻塞、不影响供给正确性；需在真机回归 checklist 评估是否将 `writeObjects` 后的供给请求统一约束到主线程。
2. 内存红线 55MB 为 release 构建口径；debug RSS 87.8MB 不构成出口标准判定依据，P1-14 收口时以 `cargo build --release` + `phys_footprint` 为准。
3. 真机回归 checklist（§2.2 六条出口）中的「Word/微信/Terminal/Slack 四类真机 100% 一致」「9 行 checklist」「15s 演示视频」仍待执行（依赖目标应用装机，承接 P0 遗留 R-1）。

### 2.4 A 线收口结果（2026-10-16，P1-14 硬指标收口）

| 子任务 | 结果 | 状态 |
|---|---|---|
| **A1 内存红线实测** | `cargo build --release` + `footprint` 真机实测：**静置 `phys_footprint = 17 MB`**（远低于 55MB 红线）；5 轮 `pbcopy/pbpaste` 负载循环后稳定 17MB，5/5 接管成功（change_count 2→10） | ✅ **达标，余量 38MB** |
| **A1-2 P2 Webview 预算结论** | 红线余量 38MB ≫ 3 窗口 Webview 理论增量（3 × 6.2MB ≈ 19MB），**P2 三窗口注入不构成内存风险**，可按 §2.3 排期直接推进 | ✅ 结论固化 |
| **A1-3 8ms 预算审计** | 5 轮负载运行日志**零「callback exceeded 8ms budget」告警**（`run_with_budget` 超限必产生 `tracing::warn`，无告警 = 无超限） | ✅ 无降级 |
| **A2 CI 门禁** | `.github/workflows/ci.yml`：macOS runner + `cargo test --lib` + 三套门禁分步执行（l1_snapshots / guardrails / hotkey_registry），含 rust-cache | ✅ 配置就绪（推送后首跑验证） |
| **A3 Master Spec v1.5 修订** | A.13 三项裁决全部固化生效：① §3.2 新增第 4 条「Probe-Guard 触发 Re-own 补挂」主方案（≤3 代封顶 + 路线 B 降级 + `RecentInternalWrites` 登记）；② `nil` 抑制语义修订（仅首个读取方生效，待装机校准 R-1）；③ P2 诊断看板需求固化（区分嗅探固化/真实供给 + 展示补挂代数）；④ D5 决议同步更新。**v1.4 移入 `pm/历史归档/`，v1.5 为唯一真源** | ✅ 裁决生效 |

**A 线出口判定**：内存红线达标（出口标准 #4 的红线项部分闭合，`phys_footprint` 静置 17MB / 负载 17MB）+ 8ms 无降级 + CI 配置就绪 + 架构口径统一。**v0.1.0 出口剩余缺口** = 真机回归（四类应用一致性 / 9 行 checklist / 15s 演示，均依赖目标应用装机）与 CI 首跑通过验证。

### 2.5 C 线 P2 进度（2026-10-16，启动批次）

| 任务 | 交付物 | 状态 |
|---|---|---|
| **P2-4** SQLite/FTS5 存储层 | `storage/db.rs`：TDD §5 完整 schema（clipboard_items + clipboard_fts trigram 外部内容表 + 三同步触发器 + served_history_log + gate_skip_metrics），WAL + synchronous=NORMAL；检索规则（≥3 字符 FTS5 trigram / 1–2 字符 LIKE 回退 LIMIT 50）；8 测试含**中文子串检索**（"架构周报总结" 命中 "周报总结"） | ✅ 8/8 |
| **P2-3** DoubleCopy 语义升级 | `double_copy.rs` 文档语义更新（双击 → Re-own + 唤醒 HUD）+ `hud.rs` 新增：`HudActionOption`（对齐 IPC Spec `HudActionOptionDto`）、`recommend_actions` 按 IM/办公/开发者三白名单生成槽位推荐，槽位 `0` 恒为 `builtin.restore-raw`；6 测试 | ✅ 6/6 |
| **P2-1/P2-2** | `hud_state.rs`：UI Spec §3.2 键盘状态机（1–4 触发 / 0·Backspace 还原原文 / ↑↓·Tab 循环高亮 / Enter 执行 / Esc 关闭且不动自动挡，Tier0 600ms·Tier1 80ms 反馈）12 测试；`hud_panel.rs`：`NonActivatingHUDPanel`（canBecomeKey=true / canBecomeMain=false，objc2 MainThreadOnly 子类）。踩坑：`NSPanel` 导出需 `NSResponder` feature、`MainThreadOnly` 须 `MainThreadMarker::alloc` | ✅ 12/12 |
| **P2-5** | `drawer.rs`：类型胶囊过滤（All/Markdown/Link/Code/Image，按 tags_json/primary_type 匹配）+ Before/After 就地预览切换 + DrawerViewModel（检索由 `HistoryDb::search` 负责）7 测试 | ✅ 7/7 |
| **P2-6** | `stack.rs`：`StackQueue`（`⌥⇧S` 开关、入队、拖拽排序、单条移除）+ 三种一键消费 `JOIN_NEWLINE`（换行拼接）/ `DEDUP_NUMBERED`（去重编号，中文验证）/ `L3_AI_SUMMARY`（P3 预留不清空队列）9 测试，语义对齐 IPC Spec `cmd_consume_stack_queue` | ✅ 9/9 |
| **P2-9** | `recipe_import.rs`：社区配方严格校验（`deny_unknown_fields` = `additionalProperties:false`、`providerTimeoutMs ≤ 8`、`regex_pipeline` 正则预编译 + flags `^[imsU]*$`、steps ≤ 16、`ownChannels` 必含 `plainText` 且去重、id/version/category/tier 枚举约束）；`llm_stream` 配方未确认审查 → `PROMPT_REVIEW_REQUIRED`（IPC Spec §3.3 错误码），非法配方 → `RECIPE_SCHEMA_INVALID`；16 测试 | ✅ 16/16 |
| **P2-8** | `ipc.rs`：IPC Spec §3 DTO（ClipboardItemDto 240 字符摘要 + tags 解析）+ 核心逻辑（search_history 全量命中分页 total 精确 / toggle_pin / set_stack_mode / consume_stack / import_recipe 错误码映射）+ 6 个 Tauri Commands 注册进 `lib.rs`（`cmd_search_history` / `cmd_toggle_pin_item` / `cmd_set_stack_mode` / `cmd_consume_stack_queue` / `cmd_import_pipe_recipe` / `cmd_list_pipe_recipes`），`IpcState` 接入 storage/stack/drawer/recipes；9 测试（测试驱动修复：total 计数被 `limit+offset` 截断 → 改全量检索内存分页） | ✅ 9/9 |
| **P2-1 收尾** | `dist/hud.html`：Micro-HUD 页面（深色圆角面板 + 1–4/0·Backspace 槽位、↑↓·Tab 高亮循环、Enter 执行、Esc 隐藏不动自动挡、Tier0 600ms 绿色就绪反馈；keydown capture + preventDefault 按键绝不泄漏）；`hud_window.rs`：2s 延迟预热常驻隐藏 `micro-hud` 窗口（IPC Spec §1）、`broadcast_summon` 唤醒广播、`HudVisibilityTracker`、3 测试；`lib.rs`：轮询线程 drain `HudSummonBus` → `pasteflow://hud-summoned` 事件 + 面板置前、`cmd_hud_visible`/`cmd_hud_trigger`（槽位 0 → R4 主线程物化还原原文 `MaterializeReason::RestoreRaw`）；全链路：双击 ⌘C⌘C → Re-own + HUD 弹出 | ✅ 3/3 |
| **P2-7** | `dist/dashboard.html`：880×600 主面板（左侧导航 200px + 内容区 680px），Tab 1 滤镜商店（4 角色预设 + 双列配方卡片 + plainTextPolicy 下拉 + llm_stream 安全审查弹窗「确认信任并导入」UI Spec §5.1）、Tab 2 诊断看板（changeCount / 今日转换 / p95 / FormatGate G1A–G5 柱状分布 / 最近 20 条回调日志 / 一键导出脱敏诊断包）、Tab 3 设置（Tier 1 权限开关含风险说明 + L3 BYOK 配置 + Ollama 探测）；`dashboard_window.rs`：按需创建 + 关闭 30s destroy（`DashboardTracker` 倒计时调度/取消）、`DiagnosticCounters`（gate 计数 + 20 条日志截断 + p50/p95/p99 nearest-rank 分位数）、脱敏导出（断言无 rawPlainText/contentHash）；`lib.rs`：托盘菜单「打开剪贴板历史/诊断看板」打开主面板、`cmd_get_diagnostic_snapshot`/`cmd_export_sanitized_report`；8 测试（测试驱动修复：percentile 改 nearest-rank、HistoryDb 无 count_all → recent(500) 统计） | ✅ 8/8 |

**P2 测试门禁**：243 全绿（lib 202 + guardrails 21 + hotkey_registry 9 + l1_snapshots 11）。**P2 交互层代码全部收口**。

**下一步**：真机冒烟（双击 ⌘C⌘C 弹 HUD + 主面板三 Tab）→ S8-E2E 按键吞吐（待辅助功能授权）→ P3（L3 流式 / 补齐 L1 滤镜 / 发布链路）。

---

## 3. P2 — 交互层：Micro-HUD / 历史抽屉 / 主面板（~1.5 周）

> 范围 = Master Spec §10.2 Must 项中的交互部分；三窗口拓扑与生命周期严格按 IPC Spec §1。

### 3.1 任务分解

| 编号 | 任务 | 依据 | 验收断言 |
|---|---|---|---|
| **P2-1** | **`micro-hud` 常驻隐藏窗口**：启动延迟 2s 预热、`visible: false` 常驻不销毁、`NonActivatingHUDPanel`（`canBecomeKey=true`）+ `WKWebView` 路由 `/#/hud` | IPC Spec §1 / D1 | 主进程内存增量 `+6.2MB` 量级（红线 < 65MB）；`⌘C⌘C` 0ms 唤醒 |
| **P2-2** | **Micro-HUD 键盘/鼠标状态机**：`1`–`4` / `0` / `↑↓` / `Tab` / `Enter` / `Esc`；Tier 0 写回提示 vs Tier 1 `enigo` 自动注入；三级定位回退（AX Caret → 鼠标 → 窗口中央） | UI Spec §3.1–3.2 | 按键 100% 不泄漏进宿主应用（P0-2 结论复用）；定位三级回退逐级验证 |
| **P2-3** | **`DoubleCopyDetector` v0.1.5 行为升级**：双击 ⌘C⌘C 从「还原原文」切换为「Re-own + 唤醒 Micro-HUD」 | TDD §3.7 / §3.4 | 双击后 HUD 弹出且推荐动作与上下文匹配 |
| **P2-4** | **SQLite 存储层**：`db.rs`（WAL + FTS5 `trigram` + 三触发器）+ `metrics.rs`（诊断聚合与脱敏周报） | TDD §5 | 中文「周报」子串检索命中；DLP 命中项 0 落表；1–2 字短查询回退 `LIKE` 生效 |
| **P2-5** | **`drawer` 历史抽屉**：360px 磁吸式、`⌥⇧V` 唤醒、搜索栏 + 类型过滤、84px 条目卡片、Before/After 就地预览 | UI Spec §4 | 检索毫秒级；置顶/取消置顶可用；预览不污染剪贴板与库状态 |
| **P2-6** | **贪吃蛇 Stack Mode**：`⌥⇧S` 开关、`StackQueue[1..N]`、拖拽排序/单条移除、三种一键消费（换行拼接 / 去重编号 / L3 预留）、屏幕角落计数角标 | UI Spec §4.2 / IPC §3.2 | `cmd_set_stack_mode` / `cmd_consume_stack_queue`（前两模式）真机闭环 |
| **P2-7** | **`main-dashboard` 主面板**：880×600 按需创建、关闭 30s 销毁；Tab 1 滤镜商店（角色预设 + 卡片 + `plainTextPolicy` 下拉）、Tab 2 诊断看板、Tab 3 设置 | UI Spec §5 / IPC Spec §1 | 关闭 30s 后 Webview 进程确实释放；诊断看板数据与 `served_history_log` 一致 |
| **P2-8** | **全量 IPC Commands/Events 联通**：`cmd_get_runtime_status` / `cmd_set_autopilot_enabled`（暂停前置物化）/ `cmd_restore_original_clipboard` / `cmd_sanitize_current_clipboard` / `cmd_execute_hud_action` / `cmd_preview_pipe_transformation` / `cmd_search_history` / `cmd_toggle_pin_item` / `cmd_import_pipe_recipe`（含 `PROMPT_REVIEW_REQUIRED`）/ 权限与诊断类；Events：`hud-summoned` / `channel-served` / `stack-updated` | IPC Spec §3–4 | 每条 Command 至少 1 条真机联调记录；错误码 `IpcErrorDto` 全枚举可触发 |
| **P2-9** | **社区配方安全导入**：`schema/pipe-v1.json` 严格校验（`additionalProperties: false`、`providerTimeoutMs ≤ 8`）、`llm_stream` 强制 Prompt 审查弹窗 | IPC Spec §5 / UI Spec §5.1 | 非法配方被拒并返回 `RECIPE_SCHEMA_INVALID`；未确认审查返回 `PROMPT_REVIEW_REQUIRED` |

### 3.2 P2 出口标准

1. 三窗口拓扑与生命周期（IPC Spec §1）逐行验收：`micro-hud` 常驻 / `drawer` 隐藏复用 / `main-dashboard` 30s 销毁。
2. UI Spec §3–5 的布局、动效、状态机逐条真机对照验收。
3. v0.1.0 全部回归用例不退化（§8.5 checklist 复跑）。

---

## 4. P3 — L3 引擎与发布收尾（~1 周）

### 4.1 任务分解

| 编号 | 任务 | 依据 | 验收断言 |
|---|---|---|---|
| **P3-1** | **L3 流式引擎**：OpenAI 兼容流式客户端（BYOK + 本地 Ollama 探测）、HUD 流式预览（首块 < 600ms）、`Esc` 中止、生成完毕一次性原子粘贴 | Master Spec §5.3 / UI Spec §3.3 | 流式首字延迟达标；`cmd_cancel_l3_stream` 即时熔断；未配置 Key 返回 `L3_CONFIG_MISSING` |
| **P3-2** | **L3 滤镜上线**：【贪吃蛇智能汇总】（会议纪要/对比表格/待办清单）、【得体打工人滤镜】 | §5.3 / §10.2 | HUD 流式预览 + 原子上屏闭环 |
| **P3-3** | **补齐 3 个 L1 滤镜**：【网页牛皮癣洗白】、【PDF 断行缝合】、【开发者瑞士军刀】 | §5.1 / §10.2 | `l1_snapshots` 新增语料覆盖；各滤镜快照断言通过 |
| **P3-4** | **全量 E2E 回归**：§8.5 两层兼容性体系复跑（CI AST/样式快照 + 真机 9 行 checklist）；§11.4 诊断看板与脱敏周报导出验收 | §8.5 / §11.4 | 全绿；`served_history_log` 变体标签齐全 |
| **P3-5** | **发布链路**：Apple Developer ID 签名 + `notarytool` 公证（D3）、Universal `.dmg`、GitHub Releases、Homebrew Cask、Tauri Updater、README（How We Work 技术叙事，§1.2 纪律） | §11.1–11.2 | Gatekeeper 拦截消除；包体 < 10MB |

**P3 进度明细（按执行顺序回写）**

| 任务 | 交付内容 | 测试 |
|---|---|---|
| **P3-1 核心** | `l3.rs`：L3 流式引擎状态机（`L3Session` Idle/Streaming/Cancelled/Completed/Failed，`feed`/`cancel` 即时熔断/`finish`）、`LlmConfig::validate()`（本地 Ollama 允许空 Key，未配置 → `L3_CONFIG_MISSING`）、`L3Provider` trait（网络层抽象 + FakeProvider 测试桩）、`AtomicPastePayload` 一次性原子粘贴（首块 < 600ms 红线断言）、`L3Recipes::with_builtin()` 四内置提示词（会议纪要/对比表格/待办清单 + 得体打工人润色，含「不得虚构」安全前缀，防 Prompt Injection）、`run_l3()`（validate → DLP 防护 → 流式聚合）、`probe_ollama()` | ✅ 11/11 |
| **P3-3** | `filters/web_cleaner.rs`【网页牛皮癣洗白】：剥离 style/class/id 属性、冗余 span、版权尾巴（©/版权所有/阅读原文/关注我们）、脚注编号，空行收敛，OnceLock 预编译正则；`filters/pdf_reflow.rs`【PDF 断行缝合】：跨行连字符合并 → 句末标点保段 → 软回车缝合（CJK 边界不插空格）→ 空行收敛；`filters/dev_swiss.rs`【开发者瑞士军刀】（`PASSTHROUGH` + `MANUAL_ONLY`）：JSON 格式化/转 TS·Go·Python·Rust 类型、curl→fetch、Unix 时间戳→ISO 8601（秒/毫秒自适应）、JWT 解码（标注 NOT verified）、Base64 解码（UTF-8 优先 + 二进制 hex 回退）；`Cargo.toml` 补 `base64`/`time` 依赖 | ✅ 26/26 |
| **P3-1 收尾** | `l3.rs` 网络层：`SseParser`（增量喂入 `data:` 行 → content delta，OpenAI `choices[0].delta.content` 与 Ollama `response` 双格式兼容，跨片段行拼接，`[DONE]` 置位）、`HttpProvider`（ureq + OpenAI 兼容 `chat/completions` SSE 流式、Bearer BYOK、8MB 流上限）、`StreamRegistry`（streamId → 原子取消标志，`cancel` 即时熔断）、`L3Runtime`（BYOK 配置 + 注册表）、`run_l3` 改回调式 Provider + `on_delta` 返回 `false` 熔断 + 真实计时 `finish_at`、`probe_ollama` 真实 HTTP（3s 超时）；`l3_ipc.rs`：`cmd_run_l3_stream`（后台线程 + 逐块 emit `pasteflow://l3-stream-chunk` 到 micro-hud/drawer）、`cmd_cancel_l3_stream`、`cmd_l3_commit_paste`（一次性原子粘贴 + 内部写入登记）、`cmd_probe_ollama`/`cmd_set_llm_config`；`dist/hud.html`：L3 流式预览展开态（呼吸灯状态条 + 140px 滚动预览 + Esc 取消/Enter 确认粘贴，流式态按键全吞不泄漏）；`hud.rs`：`HudL3Slot` 语义 + 槽位 3 得体打工人/槽位 4 智能汇总 + 唤醒载荷携带 `source_text` | ✅ 16/16 |
| **P3-3 验收** | `l1_snapshots` 补 9 条黄金快照：网页牛皮癣（样式/属性/span/脚注/版权尾巴剥离 + 空行收敛）、PDF 断行缝合（软回车缝合/硬段落保留/CJK 边界不插空格）、开发者瑞士军刀（6 类检测/JSON 确定性/curl→fetch 保 URL/时间戳 ISO 8601） | ✅ 20/20 |
| **P3-2** | 贪吃蛇智能汇总后端闭环：`l3_ipc::spawn_l3_stream` 抽出为可复用核心（validate → DLP → 提示词 → 注册流 → 后台线程 `run_l3` 逐块 emit → 完成载荷入缓存），`cmd_run_l3_stream` 薄包装转发；`ipc.rs` `cmd_consume_stack_queue` 新增 `ConsumeMode::L3AiSummary` 分支（`stack_source_text` 空行拼接队列 → `summary_to_prompt_type` 映射会议纪要/对比表格/待办清单 → `spawn_l3_stream` 返回真实 `streamId`，队列保持不清空）；HUD 得体打工人/智能汇总槽位 L3 语义已在 P3-1 收尾接入 | ✅ 303 全绿（+2 映射/拼接单测） |
| **P3-2 收尾（抽屉/贪吃蛇面板 UI）** | `drawer_window.rs`：`drawer` 窗口装配（360px 无边框透明 + 磁吸停靠右缘 `dock_metrics`，`toggle` 显隐复用不销毁，`cmd_drawer_visible`）+ 菜单「剪贴板历史」接抽屉；`dist/drawer.html`：搜索栏（FTS5 150ms 防抖）+ 5 类型胶囊过滤 + 🐍 贪吃蛇面板（队列列表拖拽排序/单条移除、`换行合并粘贴 ⌘↩`、`去重编号列表`、`✨ AI 智能汇总` 三选一下拉→`L3_AI_SUMMARY`）+ L3 流式预览（Esc 取消/↩ 一次性原子上屏）+ 84px 历史卡片（相对时间/徽章/置顶/Before-After 就地预览）；`ipc.rs`：`IpcState` 历史库与队列改共享句柄（`Arc<Mutex<..>>`），`record_copy_entry` 复制事件→入库+入队，`cmd_list_stack`/`cmd_remove_stack_item`/`cmd_reorder_stack_items` 面板命令，`consume_stack` 返回 `mergedText` 由命令层原子写剪贴板；lib.rs tick 链路接数据源 + `pasteflow://stack-updated` 广播 | ✅ 308 全绿（+5 面板/入队单测） |
| **P3-4** | 诊断链路接线（关键缺口修补）：`DiagnosticCounters` 此前零接线、看板恒空数据 → tick 链路 `record_change_count`/`record_gate_skip(gate_id)` 接入，`ProviderDeps` 持有 `diagnostics` 供 Provider 回调写入；`ServedRecordDto` 增补 `generation`（补挂代数）与 `serve_kind`（`sniffer_probe` 嗅探抢读固化 vs `real_serve` 真实粘贴供给，Master Spec §3.2 #4 / A.13 要求区分）；`gate_id`（Stage1Verdict→G1A–G5）与 `variant_label`（RouteClass→4 供给变体）映射函数 + 单测；`record_filter` 滤镜有效触发计数（§11.4 #2）；§11.4 #3 脱敏周报文件级验收单测（纯统计 JSON 含 generatedAt/historyTotal/变体/门控，绝无 rawPlainText/contentHash/snippet） | ✅ 311 全绿（+5 诊断/脱敏单测） |
| **P3 加固** | **Tier 1 自动化 + 数据持久化 + 全局热键 + IPC 接线收口**：① `tier1.rs` 辅助功能权限探测 + macOS 15.4+ `accessBehavior` 引导 + `enigo` ⌘V 按键注入 + AXUIElement 光标/鼠标三级定位；② `hotkeys.rs` 注册 5 大激活热键（⌘⇧P / ⌃⌥V / ⌃⌥D / ⌥⇧V / ⌥⇧S）；③ 数据库持久化（`app_data_dir/history.db`）+ LLM BYOK 本地持久化与回显；④ 修复 `drawer.html` Tauri 2 event namespace Bug；⑤ 主面板 Tab 3 设置与命令闭环；⑥ macOS `ActivationPolicy::Accessory` 隐藏 Dock 图标 | ✅ 335 全绿（+24 单测） |

**P3 测试门禁**：335 全绿（lib 282 + guardrails 21 + hotkey_registry 12 + l1_snapshots 20）。

**P3-4 待人工配合**：§8.5 真机 9 行 checklist（Word/微信/Terminal/Slack 路由 + 嗅探抢读后 ⌘V 仍正确分支 + 退出物化）需装机目标应用与辅助功能授权后复跑；`phys_footprint < 65MB` 红线在 release 构建后实测。

**P3-2 收尾冒烟验证（2026-10-16）**：真机启动 debug 构建（`PASTEFLOW_SMOKE=drawer` debug-only 自动展开钩子），抽屉窗口「PasteFlow 抽屉」真实创建，几何 `1098,24,360,932`（≈右缘磁吸 + 宽 360 + 高 ≈ 屏幕可用高−24，dock_metrics 命中）；浏览器渲染 `drawer.html` 6 项全过（搜索栏占位文案、5 类型胶囊默认高亮/切换、FTS5 搜索输入、贪吃蛇面板展开含三选一下拉 + 换行合并粘贴/去重编号/AI 智能汇总三按钮、按钮点击无未捕获异常、深色毛玻璃圆角 + toast 底条）。截屏权限受限故 UI 视觉以浏览器截图验收；真实数据流（复制→入库→入队→消费→⌘V）与 ⌥⇧V 热键吞吐属 S8-E2E 人工配合项。

**下一步**：P3-5 发布链路（前置：D6 品牌名裁决 + D3 Developer ID 账号）；§8.5 真机 9 行 checklist 与内存红线随装机复跑。

### 4.2 🏁 v0.1.5 发布里程碑出口标准

1. Master Spec §10.2 全部 [Must] 项交付，[Should] 项中 3 个 L1 滤镜补齐。
2. 内存红线：挂载常驻隐藏 `micro-hud` 后主进程 `phys_footprint < 65MB`（实测基线 53.53MB）。
3. 发版前真机回归 checklist 附于 Release Notes。
4. 演示材料（15 秒爆款视频 + README）符合 §1.2 对外沟通纪律。

---

## 5. 全局质量门禁与测试策略 (Quality Gates)

| 门禁 | 执行时机 | 内容 | 依据 |
|---|---|---|---|
| **G-Q1 L1 快照测试** | 每次 Commit（CI） | 10 组语料 AST/`mso-`/域名分级剥离断言 | TDD §6.1 |
| **G-Q2 护栏回归测试** | 每次 Commit（CI） | FormatGate G1A/G1B 放行与拦截、双通道抑制、CallbackBudget Panic/超时 | TDD §6.2 |
| **G-Q3 快捷键无冲突断言** | 每次 Commit（CI） | 5 个激活键位两两不冲突、不落 `FORBIDDEN_HOTKEYS` | TDD §6.3 |
| **G-Q4 性能基线断言** | 每日 / 发版前 | 对齐附录 A.12：FormatGate `< 5µs`、L1 p95 `< 5ms`、轻量净化 `< 1ms`、FrontmostApp `< 1µs` | §1.3 |
| **G-Q5 内存红线** | 发版前 | v0.1.0 `< 55MB` / v0.1.5 `< 65MB`（`phys_footprint`） | §1.3 / D1 |
| **G-Q6 真机粘贴回归 checklist** | 每次 Release | §8.5 第二层 9 行断言 | §8.5 |

## 6. 风险登记表 (Risk Register)

| 编号 | 风险 | 影响阶段 | 概率 | 缓解与止损 |
|---|---|---|---|---|
| R-1 | Slack/Discord 收到 `nil` 不回退 `plainText` | P0 / P1-7 | 中 | `<pre>` 转义源码兜底（TDD §3.4），已在设计内（本机未装 Slack，留待真机回归验证） |
| R-2 | `NonActivatingHUDPanel` + `WKWebView` 收键异常 | P0 / P2-2 | 中 | 降级隐藏 `NSTextField` 承接按键转发 JS；不阻塞 P1 |
| R-3 | macOS 15.4+ `accessBehavior` 用户设为 `ask/neverAllow` | 全程 | 中 | §8.12 引导卡（UI Spec §2.4）；`detectPatterns` 减少误触发 |
| R-4 | Provider 回调被第三方剪贴板工具并发读取/接管 | P1-8 / P2 | **高（已实证）** | exp1–4 证实嗅探者首读冻结通道内容；**缓解 = Re-own 补挂**（§1.4.2 主方案），禁反抢、`RecentInternalWrites` 防死循环 |
| R-5 | L3 流式注入误喷/撤销栈碎裂 | P3-1 | 低 | 默认 HUD 预览 + 原子粘贴；打字机注入仅 Tier 1 Opt-in 且三重安全栏 |
| R-6 | 命名冲突（`alfred-pasteflow`） | 发布前 | 低 | D6：GitHub Org `PasteFlowApp` 区分，或启用 `PasteMorph` 备选名 |
| **R-7** | **per-type 首读冻结击穿 DYNAMIC_BY_TARGET 实时分支**（exp3/4 已证实） | P1-7/P1-8 | **高（已实证）** | Re-own 补挂（exp5 已验证救活）；≤3 代封顶、补挂全通道供给后执行、内部写入短路；连续嗅探降级静态供给（路线 B） |

## 7. 交付物清单 (Deliverables Checklist)

**P0**：`spike/` E2E 冒烟脚本与结论记录（回写 Master Spec 附录 A.13）。
**P1**：`src-tauri/` 8 模块 + `handlers/` 5 算子 + `pipes/` 2 内置配方 + `schema/pipe-v1.json` + 3 套测试 + 菜单栏 App（v0.1.0 可分发构建）。
**P2**：三窗口前端（`micro-hud` / `drawer` / `main-dashboard`）+ SQLite 存储层 + 全量 IPC 联调记录。
**P3**：L3 引擎与 2 个 L3 滤镜 + 3 个补充 L1 滤镜 + 公证 `.dmg` + Release Notes（含回归 checklist）+ 演示视频与 README。

## 8. 未决事项 (Open Items)

| 编号 | 事项 | 需要拍板 | 建议 |
|---|---|---|---|
| O-1 | 排期节奏：按文档口径 ~5 周（P0+P1 约 2 周 + P2/P3 约 3 周），或压缩并行 | 产品负责人 | 建议保持串行里程碑，P2/P3 内并行（如 P2-4 存储层可与 P2-1/2 并行） |
| O-2 | 英文品牌名最终拍板（`PasteFlow` + Org `PasteFlowApp` vs `PasteMorph` / `PastePilot`） | D6 | 发布前（P3-5）必须锁定并注册 `crates.io` / `npm` / `.dev` |
| O-3 | Apple Developer ID 账号与签名预算（$99/年） | D3 | P3-5 前办妥 |
