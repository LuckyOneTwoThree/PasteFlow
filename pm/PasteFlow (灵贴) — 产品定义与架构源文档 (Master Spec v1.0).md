

## 1. 项目元信息与核心定位 (Project Metadata)

|属性|定义内容|
|---|---|
|**项目名称**|**PasteFlow**（中文代号：**灵贴**）|
|**一句话 Pitch**|_Don't just copy and paste. Transform in-flight._（懂你正在用什么软件的“自动挡”智能剪贴板）|
|**产品本质**|跨平台操作系统级数据清洗与流转中间件（OS-Level Context-Aware Clipboard Middleware）|
|**目标受众**|**大众层（80%）：** 日常重度使用办公软件、AI 助手、浏览器的白领、运营、学生、财务人员。  <br>**极客层（20%）：** 全栈开发者、AI 工程师、效率工具控、开源社区贡献者。|
|**开源北极星指标**|GitHub 10k+ Stars；安装包体积 `< 10MB`；静默常驻内存 `< 25MB`；L1 本地转换延迟 `< 5ms`。|
|**核心差异化**|告别传统剪贴板“只存不管”的仓库模式，首创 **“场景感知自动挡（Auto-Pilot）” + “微胶囊手动挡（Micro-HUD）” + “贪吃蛇收集槽（Stack Mode）”** 三位一体交互。|

## 2. 核心设计原则 (Design Principles for Agents)

后续任何子文档与代码实现必须严格遵循以下 5 条铁律：

1. **零配置开箱即用 (Zero-Config Out-of-the-Box)：** 80% 的核心高频功能必须基于本地确定性算法（L1）与系统原生免费 OCR（L2）实现，用户下载安装后第 1 秒**无需联网、无需注册、无需配置 API Key** 即可完整体验核心爽感。
    
2. **肌肉记忆零妥协 (Zero Muscle-Memory Friction)：** 绝不破坏用户几十年养成的原生 `Ctrl/Cmd + C` 与 `Ctrl/Cmd + V` 习惯。
    
3. **无焦点抢占 (Non-Activating UI)：** 任何悬浮提示、光标微胶囊（Micro-HUD）弹出时，**绝对不能抢夺当前活动窗口（如 Word、微信、IDE）的输入焦点**，确保后续模拟按键注入 100% 成功。
    
4. **本地隐私绝对优先 (100% Local-First & Privacy Guard)：** 所有剪贴板历史仅存储于本地 SQLite；内置密码管理器进程黑名单；支持在发送给外部 AI 前自动执行本地正则脱敏（DLP）。
    
5. **声明式可扩展 (Declarative Extensibility)：** 所有的转换能力均抽象为标准化的 `PipeRecipe`（滤镜配方），支持用户通过单文件 JSON/YAML 或自然语言一句话生成新滤镜，便于社区提交 PR。
    

## 3. 核心交互形态与键盘状态机 (Interaction & Input State Machine)

为了解决系统级快捷键冲突与延迟矛盾，底层键盘钩子（Keyboard Hook）与剪贴板监听器（Clipboard Watcher）按以下状态机协同工作：

### 3.1 交互面一：场景感知“自动挡” (Silent Auto-Pilot)

- **触发条件：** 用户执行普通的 `Ctrl/Cmd + C` 复制，并在目标软件中执行普通的 `Ctrl/Cmd + V` 粘贴。
    
- **系统行为：**
    
    1. **复制阶段（Capture）：** 剪贴板变动瞬间，Rust 后端捕获内容，并记录复制源窗口进程信息 `SourceApp`（如 `chrome.exe`、`Cursor`）及内容特征标签 `ContentTags`（如 `is_markdown`、`is_dirty_html`、`is_tracking_url`、`has_secret`）。
        
    2. **粘贴阶段（Intercept & Morph）：** 当监听到用户按下 `Ctrl/Cmd + V`（或剪贴板读取请求）时，系统在 `< 1ms` 内获取当前焦点窗口 `TargetApp`（如 `WINWORD.EXE`、`WeChat`、`Feishu`）。
        
    3. **规则匹配：** 若命中激活的【自动挡滤镜规则 `AutoRule(SourceApp, ContentTags, TargetApp)`】，立即在内存中原地重写剪贴板多格式缓冲区（Clipboard Formats：HTML / RTF / PlainText），随后放行系统原生粘贴指令。
        
    4. **视觉微反馈：** 粘贴完成的瞬间，在当前鼠标/文本光标右下角浮现一个无焦点半透明胶囊 Toast（停留 `800ms` 后自动淡出）：`✨ 已自动净化为 Word 标准排版`。
        

### 3.2 交互面二：光标微胶囊“手动挡” (Cursor Micro-HUD)

为了兼顾操作系统原生按键响应机制（避免拦截普通 `Ctrl+V` 造成日常粘贴卡顿），微胶囊提供**双触发通道**：

- **触发方式 A（目标端呼出 - 默认主推）：** 按下 `Alt + V` (Windows) / `Option + V` (macOS)，或按住 `Ctrl/Cmd` 连续双击 `V` 键（`Ctrl + V + V`，间隔 `< 250ms`）。
    
    - _工程细节说明：_ 当开启 `Ctrl + V + V` 模式时，底层采用低延迟按键状态机；若用户在个人设置中追求极致零延迟原生粘贴，默认引导使用 `Alt/Opt + V` 或触发方式 B。
        
- **触发方式 B（源端双击复制呼出）：** 选中内容后，按住 `Ctrl/Cmd` 连续双击 `C` 键（`Ctrl + C + C`），直接原地弹出转换微胶囊（借鉴经典划词工具直觉，零副作用）。
    
- **UI 形态与行为：**
    
    - 紧贴当前文本输入光标（Caret Position，若无法获取则回退至鼠标指针位置）弹出高颜值毛玻璃微胶囊菜单（宽 `260px`，最多展示 4 个智能推荐动作）。
        
    - **动态上下文推荐：** 菜单项根据当前剪贴板中的 `ContentType` 动态生成（见第 4 节功能矩阵）。
        
    - **极速上屏：** 用户按数字键 `1~4`、`Enter` 或鼠标点击后，若是 L1/L2 本地转换，瞬间注入目标窗口；若是 L3 AI 流式转换，微胶囊变为呼吸灯状态：`⚡ 正在转换...`，并以模拟打字机/分块粘贴方式直接输出到目标输入框。
        

### 3.3 交互面三：磁吸式“历史与贪吃蛇”抽屉 (History & Stack Drawer)

- **触发方式：** 快捷键 `Alt/Opt + Shift + V` 或鼠标停靠屏幕边缘触发。
    
- **模块 A - 时间轴历史（Timeline）：** 支持全文毫秒级搜索、按类型过滤（文本 / 图片 / 链接 / 代码 / 文件）、常用项置顶（Pin）。
    
- **模块 B - 贪吃蛇收集槽（Stack Mode / Basket）：**
    
    - 抽屉顶部设有醒目的【🐍 贪吃蛇连续收集】开关（亦可通过快捷键一键开启）。
        
    - 开启后，用户在任意窗口连续执行的 N 次 `Ctrl/Cmd + C` 不会互相覆盖，而是按序压入收集队列 `StackQueue[1..N]`，并在屏幕角落显示微型计数角标 `[🐍 已收集 5 项]`。
        
    - **一键消费：** 点击或按键执行【合并粘贴】时，支持三种输出模式：
        
        1. **换行拼接：** 自动用换行符或分割线合并所有片段并一次性粘贴。
            
        2. **去重编号：** 自动剔除重复行并生成 `1. 2. 3.` 有序列表。
            
        3. **AI 聚合整理：** 将 N 个碎片作为上下文喂给 L3 模型，直接输出结构化的会议纪要、竞品分析表或周报。
            

### 3.4 交互面四：“滤镜商店”主面板与 10 秒破冰 (Main Dashboard & Onboarding)

- **10 秒破冰（First-Run Onboarding）：** 首次安装启动绝不展示复杂设置，仅展示四张角色卡片供用户多选：
    
    - 💼 **日常办公与写作** | 📊 **财务与数据运营** | 🎓 **学生与学术研究** | 💻 **开发者与极客**
        
    - 点击【开始使用】后，系统自动激活对应角色下的默认自动挡与手动挡滤镜组合。
        
- **可视化卡片管理：** 所有规则包装为“魔法滤镜（Magic Filters）”卡片，包含：滤镜名称、适用场景图标、转换前后对比示例（Before/After Preview）、启用开关（Auto / Manual / Off）。
    
- **自然语言造滤镜（AI Pipe Builder）：** 提供一个极简输入框——_“用一句话描述你想怎么处理剪贴板内容”_（如：“把复制的杂乱地址提取出姓名、电话、省市区，并用 Tab 分隔”），系统自动生成标准 `PipeRecipe` JSON 并挂载到微胶囊中。
    

## 4. 三级引擎能力矩阵 (3-Tier Processing Engine Matrix)

系统严禁将所有任务丢给大模型。所有剪贴板处理按以下三级漏斗路由：

|引擎层级|定位与耗时要求|底层技术实现|核心内置滤镜 (Built-in Pipes)|
|---|---|---|---|
|**L1：本地闪电引擎**  <br>_(Deterministic Local Engine)_|**耗时 `< 5ms`**  <br>占日常使用 70%  <br>100% 离线、零成本|Rust 原生正则 (`regex`)、HTML/Markdown AST 解析 (`pulldown-cmark`, `scraper`)、代码结构推断 (`serde_json`)|1. **【AI 星号净化器】**：将 DeepSeek/ChatGPT 复制的 Markdown 转为带原生标题、加粗、表格的 Word/飞书富文本（HTML/RTF 双写）或微信精致纯文本。  <br>2. **【网页牛皮癣洗白】**：剥离网页复制的刺眼背景色、字号、内联样式，仅保留语义化段落、加粗与列表。  <br>3. **【PDF 断行缝合】**：自动识别中英文 PDF 复制产生的硬回车断行与连字符，合并为通顺自然段。  <br>4. **【社媒链接瘦身】**：自动剥离淘宝、B站、小红书、京东等分享文本中的追踪参数（如 `spm=`、`vd_source=`）与冗长口令。  <br>5. **【开发者瑞士军刀】**：JSON 格式化与转 TS/Go/Python/Rust 类型定义；cURL 转 `fetch`/代码；时间戳与 JWT/Base64 实时解码预览。  <br>6. **【DLP 隐私脱敏盾】**：自动识别 API Key (`sk-...`)、数据库连接串、私钥、手机号、身份证号并替换为安全占位符 `<REDACTED>`。|
|**L2：系统原生视觉引擎**  <br>_(Native OS Vision Engine)_|**耗时 `< 300ms`**  <br>占日常使用 20%  <br>100% 离线、零安装包膨胀|macOS: `Vision Framework` (`VNRecognizeTextRequest`)  <br>Windows: `Windows.Media.Ocr`  <br>配合本地二维坐标聚类算法还原表格行列|1. **【截图秒变 Excel】**：识别剪贴板图片中的文本块与坐标边界框（Bounding Box），按行列对齐还原为 TSV（制表符分隔）或 Markdown Table，贴入 Excel 自动分列。  <br>2. **【离线极速 OCR】**：截图或扫描件直接提取为纯净可编辑文本。  <br>3. **【二维码与色板提取】**：复制图片自动解析内含二维码链接及主视觉 Hex/Tailwind 色值。|
|**L3：流式 AI 与 Agent 引擎**  <br>_(Streaming AI & MCP Engine)_|**首字耗时 `< 600ms`**  <br>占日常使用 10%  <br>支持 BYOK / 免费 Flash 模型 / 本地 Ollama|OpenAI 兼容流式客户端（原生预设千问 Flash、智谱 GLM-Flash、Gemini Flash、DeepSeek 及本地 Ollama）；内置 MCP Server|1. **【得体打工人滤镜】**：将大白话或情绪化草稿润色为专业、高情商的职场邮件或即时通讯回复。  <br>2. **【贪吃蛇智能汇总】**：将收集槽中的多条零散复制记录聚合提炼为结构化摘要或待办清单。  <br>3. **【跨语境翻译】**：复制外文段落自动翻译为地道母语，或将中文直接转为地道英文 Commit Message。  <br>4. **【UI 截图转代码 / 公式识别】**：调用多模态视觉模型将截图还原为 LaTeX 数学公式或前端 Tailwind 组件。  <br>5. **【MCP 上下文桥接】**：作为本地 MCP Server 向 Cursor、Trae、Claude Desktop 暴露安全脱敏后的剪贴板历史与收集槽数据。|

## 5. 技术栈与系统架构规范 (Technical Architecture & Stack)

Coding Agent 在初始化项目与编写架构设计时，必须严格采用以下技术选型：

- **跨端桌面框架：** **Tauri 2.0**（严禁使用 Electron，确保极致轻量）。
    
- **后端核心层（Rust）：**
    
    - 剪贴板监听与多格式读写：`arboard` / `clipboard-rs` + 平台原生扩展（支持 Text, HTML, RTF, Image, FileList）。
        
    - 全局快捷键与输入模拟：`tauri-plugin-global-shortcut`、`rdev`（按键序列监听）、`enigo`（跨平台按键/粘贴注入）。
        
    - 活动窗口与进程感知：`active-win-pos-rs` + 原生系统 API（获取 `process_name`, `bundle_id`, `window_title` 及输入光标 `caret_rect`）。
        
    - 无焦点窗口控制：macOS 通过 `objc2` / `cocoa` 设置 `NSPanel` 与 `NSWindowStyleMaskNonActivatingPanel`；Windows 通过 `windows-rs` 设置 `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW`。
        
    - 本地存储与全文检索：`rusqlite` (SQLite3，开启 FTS5 全文索引引擎) + 本地图片缩略图缓存。
        
- **前端视图层（TypeScript + React 19）：**
    
    - 构建工具与样式：`Vite` + `Tailwind CSS` + `shadcn/ui` + `Framer Motion`（用于微胶囊丝滑弹出动画）。
        
    - 多窗口架构：Tauri 多 Webview 实例管理——`main-dashboard`（按需创建/销毁以节省内存）、`micro-hud`（常驻隐藏，0ms 唤醒）、`drawer`（侧边抽屉）。
        

### 系统数据流转图 (Data Flow Pipeline)

```
[System Clipboard Event] 
       │
       ▼
[Rust Watcher & Loop Guard] ──(Ignore if Self-Write Hash matches)──► [Drop]
       │
       ▼
[Security & Process Filter] ──(Ignore if SourceApp in Blacklist: 1Password/Bitwarden)──► [Drop]
       │
       ▼
[Content Analyzer & Tagger] ──► Extract: MIME types, SourceApp, ContentTags (JSON/MD/URL/Table/Secret)
       │
       ├──► [SQLite FTS5 Storage / Stack Mode Queue]
       │
       └──► [Trigger Router]
               ├─► (Path A: Native Ctrl+V pressed) ──► Check Active Window (`TargetApp`)
               │     └─► Match `AutoRule`? ──(Yes)──► Run L1 Engine (<5ms) ──► Rewrite Clipboard Buffer ──► Pass-through Paste + Show Mini Toast
               │
               └─► (Path B: Micro-HUD Triggered) ──► Show Non-Activating HUD at Cursor
                     └─► User Selects Pipe ──► Execute L1 / L2 / L3 Pipe ──► Inject Result to TargetApp
```

## 6. 核心数据结构与协议定义 (Core Data Schemas)

为了保证前后端与插件生态的严谨对接，以下为核心实体的标准 Schema 定义：

### 6.1 剪贴板数据条目 (`ClipboardItem`)

```typescript
export interface ClipboardItem {
  id: string;                  // UUID v7 (按时间有序)
  createdAt: number;           // Unix Timestamp (ms)
  contentHash: string;         // BLAKE3 Hash (用于防死循环与去重)
  primaryType: 'text' | 'html' | 'rtf' | 'image' | 'files';
  formats: {
    plainText?: string;
    html?: string;
    rtf?: string;
    imagePath?: string;        // 本地缓存路径
    filePaths?: string[];
  };
  sourceContext: {
    appName: string;           // e.g., "Google Chrome", "Cursor"
    processName: string;       // e.g., "chrome.exe"
    windowTitle?: string;      // e.g., "DeepSeek - 探索未至之境"
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
  isPinned: boolean;
  inStack: boolean;            // 是否处于贪吃蛇收集槽中
}
```

### 6.2 声明式滤镜配方协议 (`PipeRecipe`)

所有内置及社区贡献的滤镜均遵循此结构（存放于 `pipes/*.json`）：

```json
{
  "$schema": "https://pasteflow.dev/schema/pipe-v1.json",
  "id": "builtin.markdown-to-office",
  "name": "AI 星号净化器 (Markdown 转办公格式)",
  "description": "将 AI 助手复制的带 # 和 ** 的文本，自动转换为 Word/飞书原生加粗与标准表格",
  "author": "PasteFlow Official",
  "version": "1.0.0",
  "icon": "Sparkles",
  "category": "office",
  "tier": "L1_LOCAL",
  "trigger": {
    "mode": "AUTO_AND_MANUAL",
    "matchContentTags": ["markdown"],
    "matchSourceApps": ["*"],
    "matchTargetApps": ["WINWORD.EXE", "Microsoft Word", "Feishu", "Lark", "DingTalk", "WPS Office"]
  },
  "processor": {
    "engine": "builtin_rust_handler",
    "handlerName": "markdown_to_rich_office",
    "fallbackPrompt": null
  },
  "output": {
    "writeFormats": ["html", "rtf", "plainText"],
    "toastMessage": "✨ 已自动净化为办公文档格式"
  }
}
```

### 6.3 MCP Server 暴露接口规范 (`MCP Integration`)

当开启本地 MCP 服务时，向外部桌面 AI Agent 暴露以下标准能力：

- **Resources:**
    
    - `clipboard://latest`：获取当前最新一条剪贴板内容（默认自动经过 L1 DLP 隐私脱敏处理）。
        
    - `clipboard://stack`：获取当前“贪吃蛇收集槽”中聚合的所有多模态/文本片段。
        
    - `clipboard://history?limit=10`：获取最近 N 条复制记录。
        
- **Tools:**
    
    - `run_clipboard_pipe(pipe_id, input_text)`：允许外部 Agent 调用 PasteFlow 内置的转换与清洗管道。
        
    - `push_to_clipboard(content, format)`：允许外部 Agent 将生成结果直接按富文本或特定格式写入系统剪贴板。
        

## 7. 关键技术壁垒与避坑指南 (Critical Edge Cases & Engineering Guardrails)

在编写技术设计文档（TDD）和实现代码时，必须重点攻克以下 4 个系统级难点：

1. **防剪贴板死循环（Infinite Loop Guard）：**
    
    - _问题：_ 软件修改剪贴板写回系统时，会再次触发自身的 `Clipboard Watcher`。
        
    - _强制方案：_ 每次由 PasteFlow 内部写回剪贴板前，计算内容的 `BLAKE3 Hash` 存入内存环形缓冲区 `RecentInternalWrites`，并在写入时附带自定义私有剪贴板格式标识（如 `application/x-pasteflow-internal`）。Watcher 检测到该标识或匹配 Hash 时直接短路跳过。
        
2. **输入光标坐标获取与回退机制（Caret Position Fallback）：**
    
    - _问题：_ 在不同应用中（如原生 Win32 应用 vs Electron 应用 vs 浏览器 Canvas），操作系统的 Accessibility API 不一定能 100% 拿到文本闪烁光标（Caret）的精确屏幕坐标。
        
    - _强制方案：_ 采用三级回退策略：优先通过 `UIAutomation` (Win) / `AXUIElement` (Mac) 获取精确输入光标坐标 \rightarrow 失败则回退至当前鼠标指针（Mouse Cursor）坐标偏移 `(12px, 12px)` 处 \rightarrow 若鼠标在屏幕外则回退至活动窗口正中央偏下位置。
        
3. **跨软件粘贴格式兼容性（Clipboard MIME Negotiation）：**
    
    - _问题：_ 往 Word 贴表格和往微信贴文字所需的剪贴板底层格式截然不同。
        
    - _强制方案：_ 写入富文本时必须执行“多通道同时写入（Multi-Format Write）”——同时在系统剪贴板缓冲区写入标准 `CF_HTML` / `public.html`、`CF_RTF` / `public.rtf` 以及降级排版后的 `CF_UNICODETEXT`。这样目标软件若支持富文本（如 Word/飞书）会自动读取 HTML/RTF，若仅支持纯文本（如微信聊天框/记事本）会自动读取美化后的纯文本。
        
4. **隐私安全绝对红线（Security Blacklist）：**
    
    - _强制方案：_ 内置不可关闭的敏感源过滤机制。当检测到复制源进程为 `1Password`、`Bitwarden`、`KeePass`、`Keychain Access` 或包含系统级标准隐私标记（macOS `org.nspasteboard.ConcealedType` / Windows `ExcludeClipboardContentFromMonitorProcessing`）时，完全忽略该次复制，不进内存、不落盘、不触发任何管道。
        

## 8. 版本路线图与 MVP 严格边界 (Roadmap & MVP Scope)

为了保证第一版（v0.1.0）能在 **2~3 周内** 高质量交付并具备冲击 GitHub Trending 的完成度，严格执行以下范围划分：

### Phase 1: MVP v0.1.0（核心引爆版 —— 必须 100% 完成）

- **[Must]** 基于 Tauri 2.0 + Rust 的跨平台底座（macOS + Windows），实现防死循环监听、进程识别与无焦点 `Micro-HUD` 弹窗。
    
- **[Must]** 跑通“自动挡（Auto-Pilot）”拦截重写闭环与 `Alt/Opt + V`（及 `Ctrl+C+C`）手动微胶囊闭环。
    
- **[Must]** 内置 **6 个 L1 本地神级滤镜**：AI Markdown 转办公富文本、网页样式洗白、PDF 断行缝合、社媒链接瘦身、JSON/代码快速转换、DLP 敏感密钥自动脱敏。
    
- **[Must]** 内置 **L2 系统原生离线 OCR**：实现零配置截图提取文字与截图转 Excel 表格（TSV）。
    
- **[Must]** 基础侧边栏抽屉（含 SQLite FTS5 历史搜索 + **贪吃蛇连续收集槽 Stack Mode**）。
    
- **[Must]** 接入 OpenAI 兼容流式接口（预设智谱 GLM-Flash / 千问 Flash / DeepSeek / Ollama），支持“贪吃蛇 AI 汇总”与“职场高情商润色”。
    
- **[Must]** 10 秒角色破冰引导页 + 滤镜卡片开关面板。
    

### Phase 2: v0.2.0（开源生态与极客破圈版）

- **[Should]** 上线“一句话自然语言生成滤镜（AI Pipe Builder）”及 `.json` 滤镜配方导入/导出机制，建立 GitHub `awesome-pasteflow-pipes` 社区仓库。
    
- **[Should]** 正式集成 **Local MCP Server**，一键生成 Cursor / Claude Desktop / Trae 对接配置。
    
- **[Should]** Linux (X11 / Wayland) 平台适配。
    

### Phase 3: v0.3.0+（跨端与高阶演进）

- **[Could]** 局域网端到端加密（WebRTC / Local mDNS）跨设备剪贴板空投（iOS/Android 扫码即传）。
    
- **[Could]** 复杂多步链式管道（Pipe Chaining，如：截图 OCR \rightarrow 自动翻译 \rightarrow 转为 Markdown 表格）。
    

## 9. 下游 Agent 研发指令指南 (Instructions for Downstream Agents)

当将本文档输入给后续 Agent 推进研发时，建议按以下顺序拆解任务：

1. **生成《UI/UX 详细交互与视觉规范 (UI Spec)》：** 重点定义 `Micro-HUD` 的像素级尺寸、毛玻璃明暗色模式（Dark/Light Mode）Token、键盘导航高亮态，以及“贪吃蛇模式”的悬浮角标动画。
    
2. **生成《Rust 核心引擎技术设计文档 (TDD)》：** 重点明确 `ClipboardManager`、`WindowObserver`、`KeyhookStateMachine`、`PipeExecutor` 四个核心 Rust Module 的 Trait 接口定义、线程模型（异步 Tokio Runtime 与系统主线程 UI 事件循环通信）及跨平台条件编译（`#[cfg(target_os = "macos")]` / `#[cfg(target_os = "windows")]`）实现细节。
    
3. **生成《前端状态与 Tauri IPC 接口契约 (IPC Spec)》：** 明确 Rust 后端与 React 前端之间的 `Tauri Commands` 与 `Events` 载荷格式。
    
4. **脚手架初始化与敏捷迭代：** 严格按照 Phase 1 MVP 顺序，先跑通无 UI 的 Rust 剪贴板拦截与 L1 转换单元测试，再挂载 `Micro-HUD` 与主界面。
    

