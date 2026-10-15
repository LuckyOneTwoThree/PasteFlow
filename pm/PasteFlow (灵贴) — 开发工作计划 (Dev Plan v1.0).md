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
