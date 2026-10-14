# PasteFlow (灵贴) — 前端状态与 Tauri IPC 接口契约 (IPC Spec v1.0)

> **文档状态：** 生效中（Aligned with Master Spec v1.4）
> **修订日期：** 2026-10-14
> **上游真源：** [[PasteFlow (灵贴) — 产品定义与架构源文档 (Master Spec v1.4)]]
> **关联文档：** [[PasteFlow (灵贴) — Rust 核心引擎技术设计文档 (TDD v1.0)]] · [[PasteFlow (灵贴) — UI与交互设计规范 (UI Spec v1.0)]]

## 1. 多窗口拓扑与生命周期契约 (Multi-Window Topology & Lifecycle)

根据 Master Spec v1.4 §6.1 与决议 D1，系统严禁使用 Tauri `unstable` 的单窗口多 Webview 特性，统一采用稳定版独立 `WebviewWindow` API，并按版本阶段严格控制进程生命周期：

| 窗口 Label | 交付阶段 | 宿主窗口类型 | 创建与销毁生命周期策略 | 路由入口 |
|---|---|---|---|---|
| `tray-menu` | **v0.1.0** | 原生 `NSStatusItem` / `NSMenu` | 随主进程启动常驻，零 Webview 开销 | 原生菜单构建器 |
| `toast-overlay` | **v0.1.0** | 原生无焦点 `NSPanel` (`ignoresMouseEvents=true`) | 随主进程启动常驻隐藏，接管 `260ms` 消抖后显示 `800ms` | 原生 CoreAnimation 层 |
| `micro-hud` | **v0.1.5** | `NonActivatingHUDPanel` (`NSPanel` 子类 + `WKWebView`) | 主进程启动后延迟 `2s` 在后台预热创建，保持 `visible: false` **常驻不销毁**（实测主进程内存增量仅 `+6.20 MB`），确保 `⌘C⌘C` `0ms` 唤醒 | `/#/hud` |
| `drawer` | **v0.1.5** | 独立 `WebviewWindow` (`decorations: false`, `transparent: true`) | 首次按下 `⌥⇧V` 时创建，关闭时隐藏（`hide()`） | `/#/drawer` |
| `main-dashboard` | **v0.1.5** | 标准 `WebviewWindow` (`880×600`, 带标题栏) | **按需创建**；用户点击关闭按钮后隐藏并启动 `30s` 倒计时，期间未再次打开则调用 `window.destroy()` 彻底释放 WebKit 渲染进程 | `/#/dashboard` |

## 2. 前后端共享核心数据类型契约 (`src/shared/ipc-types.ts`)

以下 TypeScript 类型定义与 Rust 端 `serde` 结构体（`#[serde(rename_all = "camelCase")]`）严格一一对应：

```typescript
export type TierLevel = 'TIER_0_ZERO_PERMISSION' | 'TIER_1_ACCESSIBILITY';

export type PlainTextPolicy =
  | 'DYNAMIC_BY_TARGET'
  | 'PASSTHROUGH'
  | 'CLEANED'
  | 'SANITIZED_OPT_IN';

export type ContentTag =
  | 'markdown'
  | 'dirty_html'
  | 'url_with_tracker'
  | 'pdf_broken_lines'
  | 'json'
  | 'curl'
  | 'code'
  | 'contains_secret'
  | 'table_image';

export type GateSkipReason =
  | 'G1_COMPLEX_OBJECT'
  | 'G2_MULTI_ITEM'
  | 'G3_FILE_URL'
  | 'G4_LEGACY_ONLY'
  | 'G5_OVERSIZE'
  | 'DLP_SECRET_BYPASS';

export type AppliedVariant =
  | 'rich_office'
  | 'im_cleaned_text'
  | 'raw_passthrough'
  | 'html_suppressed'
  | 'probe_guard_passthrough';

export interface ServedRecordDto {
  timestamp: number;
  channel: 'html' | 'rtf' | 'plainText';
  frontmostBundleId: string;
  frontmostAppName: string;
  appliedVariant: AppliedVariant;
  durationUs: number;
  degradedTimeout: boolean;
}

export interface ClipboardItemDto {
  id: string;
  createdAt: number;
  contentHash: string;
  primaryType: 'text' | 'html' | 'rtf' | 'image' | 'files';
  previewText: string;                  // 截断至前 300 字符供列表高速渲染
  textByteLength: number;
  sourceContext: {
    appName: string;
    bundleId?: string;
    processName: string;
    resolutionConfidence: 'exact' | 'stale';
  };
  tags: ContentTag[];
  plainTextPolicy: PlainTextPolicy;
  isPinned: boolean;
  inStack: boolean;
  ownership: {
    owned: boolean;
    changeCountAtOwn: number;
    materialized: boolean;
    lastServed?: ServedRecordDto;
  };
}

export interface HudActionOptionDto {
  slotKey: '1' | '2' | '3' | '4' | '0';
  pipeId: string;                       // 'builtin.restore-raw' 对应槽位 '0'
  title: string;
  subtitle?: string;
  tier: 'L1_LOCAL' | 'L2_VISION' | 'L3_STREAMING_AI';
  icon: string;
}

export interface HudSummonPayloadDto {
  itemId: string;
  sourceAppName: string;
  byteSizeLabel: string;                // e.g. "2.4 KB"
  tags: ContentTag[];
  anchorPoint: {
    x: number;
    y: number;
    locatingSource: 'AX_CARET' | 'MOUSE_CURSOR' | 'WINDOW_CENTER';
  };
  recommendedActions: HudActionOptionDto[];
}
```

## 3. Tauri Commands 接口契约 (Frontend → Backend RPC)

所有 Command 统一返回 `Result<T, IpcErrorDto>`，错误结构如下：

```typescript
export interface IpcErrorDto {
  code:
    | 'CONCURRENT_CLIPBOARD_MODIFIED' // TOCTOU 校验失败，剪贴板已被新复制接替
    | 'GATE_REJECTED'                 // 被 FormatGate G1–G5 拦截
    | 'CALLBACK_TIMEOUT'              // 超出 8ms 预算已降级
    | 'ITEM_NOT_FOUND'                // 历史条目不存在
    | 'L3_CONFIG_MISSING'             // 未配置 API Key 或本地 Ollama 未启动
    | 'L3_PROVIDER_ERROR'             // 模型请求异常
    | 'INTERNAL_ERROR';
  message: string;
}
```

### 3.1 核心剪贴板与转换控制 Commands（v0.1.0 & v0.1.5 共用）

| Command 名称 | 请求参数 (`Args`) | 返回载荷 (`Return`) | 行为规范 |
|---|---|---|---|
| `cmd_get_runtime_status` | `{}` | `RuntimeStatusDto` | 获取当前自动挡总开关、当前持有状态、今日有效转换次数、最近一次通道供给详情与 macOS 15.4+ `accessBehavior` 状态。 |
| `cmd_set_autopilot_enabled` | `{ enabled: boolean }` | `{ enabled: boolean }` | 开启或暂停自动挡主链路；暂停时立即释放当前 DataProvider 强引用。 |
| `cmd_restore_original_clipboard` | `{ itemId?: string }` | `{ restoredChangeCount: number }` | 将当前（或指定 `itemId`）条目的 `original.plainText` 原子写回系统剪贴板并登记 `LoopGuard`，触发 `↩️ 已还原为原始未转换文本` Toast。 |
| `cmd_sanitize_current_clipboard` | `{}` | `{ redactedCount: number }` | 显式将当前剪贴板文本中的高置信度密钥替换为 `<REDACTED>` 并写回剪贴板。 |
| `cmd_execute_hud_action` | `{ itemId: string, pipeId: string }` | `{ mode: 'PASTED_AUTO' \| 'COPIED_WAIT_PASTE' \| 'L3_STREAM_STARTED', streamId?: string }` | 在 `Micro-HUD` 中按下 `1`–`4` 时调用：若为 L1/L2 则立即计算并写回剪贴板（Tier 1 自动调用 `enigo` 发送 `⌘V`，Tier 0 返回 `COPIED_WAIT_PASTE`）；若为 L3 则启动后台 SSE 流并返回 `streamId`。 |
| `cmd_preview_pipe_transformation` | `{ itemId: string, pipeId: string }` | `{ beforeText: string, afterHtml?: string, afterPlainText: string, elapsedUs: number }` | 供历史抽屉或滤镜商店卡片展示 Before/After 预览。仅在内存中临时调用 L1 算子，**绝不修改系统剪贴板或污染数据库状态**。 |

### 3.2 历史抽屉、贪吃蛇与 L3 流式 Commands（v0.1.5）

| Command 名称 | 请求参数 (`Args`) | 返回载荷 (`Return`) | 行为规范 |
|---|---|---|---|
| `cmd_search_history` | `{ query: string, tagFilter?: ContentTag, limit: number, offset: number }` | `{ items: ClipboardItemDto[], total: number }` | 基于 SQLite FTS5 `MATCH` 毫秒级检索历史记录。 |
| `cmd_toggle_pin_item` | `{ itemId: string, pinned: boolean }` | `{ success: boolean }` | 置顶或取消置顶历史记录。 |
| `cmd_set_stack_mode` | `{ active: boolean }` | `{ active: boolean, count: number }` | 开启或关闭贪吃蛇连续收集槽（`⌥⇧S`）。 |
| `cmd_consume_stack_queue` | `{ mode: 'JOIN_NEWLINE' \| 'DEDUP_NUMBERED' \| 'L3_AI_SUMMARY', summaryPromptType?: 'MEETING_NOTES' \| 'COMPARISON_TABLE' \| 'TODO_LIST' }` | `{ consumedCount: number, streamId?: string }` | 一键消费贪吃蛇队列：前两种模式同步合并写入剪贴板并清空队列；`L3_AI_SUMMARY` 启动流式聚合。 |
| `cmd_cancel_l3_stream` | `{ streamId: string }` | `{ cancelled: boolean }` | 用户在 HUD 流式预览态按下 `Esc` 时立即中止 HTTP 流。 |

### 3.3 滤镜配置、权限与可观测性诊断 Commands

| Command 名称 | 请求参数 (`Args`) | 返回载荷 (`Return`) | 行为规范 |
|---|---|---|---|
| `cmd_list_pipe_recipes` | `{}` | `PipeRecipeWithStateDto[]` | 返回所有内置及用户自定义滤镜配方及其当前开关（`Auto / Manual / Off`）与 `plainTextPolicy`。 |
| `cmd_update_pipe_config` | `{ pipeId: string, mode: 'AUTO' \| 'MANUAL' \| 'OFF', plainTextPolicy?: PlainTextPolicy }` | `{ updated: boolean }` | 更新单张滤镜卡片的触发模式或纯文本通道策略。 |
| `cmd_get_permission_status` | `{}` | `{ tier: TierLevel, accessibilityGranted: boolean, pasteboardAccessBehavior: 'ALWAYS_ALLOW' \| 'ASK' \| 'NEVER_ALLOW' }` | 查询当前 Tier 等级与系统权限状态。 |
| `cmd_open_system_privacy_pane` | `{ pane: 'PASTEBOARD' \| 'ACCESSIBILITY' }` | `{ opened: boolean }` | 引导跳转 macOS 系统隐私设置对应页面。 |
| `cmd_get_diagnostic_snapshot` | `{}` | `DiagnosticSnapshotDto` | 获取当前 `changeCount`、最近 20 条 `ServedRecordDto`、G1–G5 拦截统计与回调耗时分位数（p50/p95/p99）。 |
| `cmd_export_sanitized_report` | `{ targetPath: string }` | `{ savedPath: string }` | 导出完全不含任何剪贴板正文的纯统计 JSON 周报文件。 |

## 4. Tauri Events 事件广播契约 (Backend → Frontend Push)

Rust 后端通过 `app_handle.emit_to(window_label, event_name, payload)` 定向推送事件，避免唤醒无关隐藏窗口：

| 事件名称 (`Event Name`) | 目标接收窗口 | 触发时机 | 载荷类型 (`Payload`) |
|---|---|---|---|
| `pasteflow://toast-trigger` | `toast-overlay` | 自动挡接管完成且过完 `260ms` 消抖窗口（未触发 `⌘C⌘C`），或双击还原原文时 | `{ variant: 'DYNAMIC_READY' \| 'CLEANED' \| 'RESTORED_RAW' \| 'REDACTED', message: string, mouseX: number, mouseY: number }` |
| `pasteflow://hud-summoned` | `micro-hud` | 用户按下 `⌘C⌘C`（`<250ms`）或 `⌃⌥V` / `Ctrl+Alt+V` 时 | `HudSummonPayloadDto`（前端收到后立即渲染列表并由 Rust 端调用 `panel.orderFrontRegardless()` 0ms 弹出） |
| `pasteflow://channel-served` | `main-dashboard` (若打开) | 每次 `provideDataForType` 完成真实目标应用供给后（已过滤 `<150ms` 抢读） | `ServedRecordDto` |
| `pasteflow://stack-updated` | `drawer` | 贪吃蛇收集槽开启期间每次新增复制片段或消费清空时 | `{ active: boolean, items: ClipboardItemDto[] }` |
| `pasteflow://l3-stream-chunk` | `micro-hud` / `drawer` | L3 流式大模型每返回一段 SSE Delta Token 时 | `{ streamId: string, delta: string, accumulatedText: string, done: boolean, firstTokenLatencyMs?: number }` |

## 5. `PipeRecipe` 标准化 JSON Schema (`schema/pipe-v1.json`)

所有内置配方（`pipes/*.json`）与社区提交的 PR 必须通过以下 JSON Schema 校验（全层级强制 `additionalProperties: false`，`providerTimeoutMs` 上限锁死 `8ms`）：

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://raw.githubusercontent.com/PasteFlowApp/pasteflow/main/schema/pipe-v1.json",
  "title": "PasteFlow PipeRecipe Schema v1.4",
  "type": "object",
  "additionalProperties": false,
  "required": [
    "$schema",
    "id",
    "name",
    "description",
    "author",
    "version",
    "icon",
    "category",
    "tier",
    "trigger",
    "processor",
    "output"
  ],
  "properties": {
    "$schema": { "type": "string", "format": "uri" },
    "id": { "type": "string", "pattern": "^[a-z0-9_]+(\\.[a-z0-9_-]+)+$" },
    "name": { "type": "string", "minLength": 2, "maxLength": 64 },
    "description": { "type": "string", "minLength": 4, "maxLength": 240 },
    "author": { "type": "string", "minLength": 1, "maxLength": 64 },
    "version": { "type": "string", "pattern": "^\\d+\\.\\d+\\.\\d+$" },
    "icon": { "type": "string" },
    "category": {
      "type": "string",
      "enum": ["office", "developer", "cleaning", "privacy", "vision", "ai_writing"]
    },
    "tier": {
      "type": "string",
      "enum": ["L1_LOCAL", "L2_VISION", "L3_STREAMING_AI"]
    },
    "trigger": {
      "type": "object",
      "additionalProperties": false,
      "required": ["mode", "matchContentTags", "matchSourceApps", "ownChannels", "minConfidence"],
      "properties": {
        "mode": {
          "type": "string",
          "enum": ["AUTO_AND_MANUAL", "MANUAL_ONLY", "OFF"]
        },
        "matchContentTags": {
          "type": "array",
          "items": {
            "type": "string",
            "enum": [
              "markdown",
              "dirty_html",
              "url_with_tracker",
              "pdf_broken_lines",
              "json",
              "curl",
              "code",
              "contains_secret",
              "table_image"
            ]
          },
          "minItems": 1
        },
        "matchSourceApps": {
          "type": "array",
          "items": { "type": "string" },
          "minItems": 1
        },
        "ownChannels": {
          "type": "array",
          "items": { "type": "string", "enum": ["html", "rtf", "plainText"] },
          "contains": { "const": "plainText" },
          "minItems": 1,
          "uniqueItems": true
        },
        "minConfidence": {
          "type": "number",
          "minimum": 0.0,
          "maximum": 1.0
        }
      }
    },
    "processor": {
      "type": "object",
      "additionalProperties": false,
      "required": ["engine", "handlerName", "fallbackPrompt", "providerTimeoutMs"],
      "properties": {
        "engine": {
          "type": "string",
          "enum": ["builtin_rust_handler", "regex_pipeline", "llm_stream"]
        },
        "handlerName": { "type": "string" },
        "fallbackPrompt": { "type": ["string", "null"] },
        "providerTimeoutMs": {
          "type": "integer",
          "minimum": 1,
          "maximum": 8
        }
      }
    },
    "output": {
      "type": "object",
      "additionalProperties": false,
      "required": ["channels", "plainTextPolicy", "preserveOriginal", "toastMessages"],
      "properties": {
        "channels": {
          "type": "object",
          "additionalProperties": false,
          "required": ["plainText"],
          "properties": {
            "html": {
              "type": "object",
              "additionalProperties": false,
              "required": ["handler"],
              "properties": {
                "handler": { "type": "string" },
                "suppressForTargetBundles": {
                  "type": "array",
                  "items": { "type": "string" }
                }
              }
            },
            "rtf": {
              "type": "object",
              "additionalProperties": false,
              "required": ["handler"],
              "properties": {
                "handler": { "type": "string" }
              }
            },
            "plainText": {
              "type": "object",
              "additionalProperties": false,
              "required": ["defaultHandler"],
              "properties": {
                "defaultHandler": { "type": "string" },
                "imTargetHandler": { "type": "string" },
                "imTargetBundles": {
                  "type": "array",
                  "items": { "type": "string" }
                }
              }
            }
          }
        },
        "plainTextPolicy": {
          "type": "string",
          "enum": ["DYNAMIC_BY_TARGET", "PASSTHROUGH", "CLEANED", "SANITIZED_OPT_IN"]
        },
        "preserveOriginal": { "type": "boolean" },
        "toastMessages": {
          "type": "object",
          "additionalProperties": false,
          "properties": {
            "DYNAMIC_BY_TARGET": { "type": "string" },
            "PASSTHROUGH": { "type": "string" },
            "CLEANED": { "type": "string" },
            "SANITIZED_OPT_IN": { "type": "string" }
          }
        }
      }
    }
  }
}
```
