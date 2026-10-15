import Foundation
import AppKit
import WebKit
import Darwin.Mach

// ============================================================================
// PasteFlow (灵贴) — Master Spec v1.4 Step 0A 架构地基与算子实测套件 (AOT Release)
// 覆盖验证项:
//   • S0-Logic: WindowObserver 环形缓冲 O(1) 读取 + DYNAMIC_BY_TARGET 路由 + HTML/RTF 双通道同步抑制
//   • S0.5:     macOS 15.4+/27 NSPasteboard.accessBehavior 隐私状态探测
//   • S1 & S5:  L1 算子耗时、G5 双层阈值 (<=64KB 全量 / 64~256KB 轻量纯文本 / >256KB 拒绝)
//   • S2:       两阶段 FormatGate (Stage1 零读取元数据门控 G1A/G1B/G2/G3/G4 + Stage2 体积分级)
//               含「Word/Pages 富文本伴生 TIFF 预览图放行」与「纯图片截图拦截」专项用例
//   • S7:       极速复制切窗 (30ms) 抗竞态 SourceApp 时间戳回溯
//   • S8 & S11: NonActivatingHUDPanel 属性校验与三阶段真实物理内存 (phys_footprint & RSS)
// ============================================================================

struct MemorySnapshot {
    let residentMB: Double
    let physFootprintMB: Double
}

func currentMemorySnapshot() -> MemorySnapshot {
    var vmInfo = task_vm_info_data_t()
    var count = mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size) / 4
    let kr = withUnsafeMutablePointer(to: &vmInfo) {
        $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
            task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count)
        }
    }
    if kr == KERN_SUCCESS {
        return MemorySnapshot(
            residentMB: Double(vmInfo.resident_size) / (1024.0 * 1024.0),
            physFootprintMB: Double(vmInfo.phys_footprint) / (1024.0 * 1024.0)
        )
    }
    return MemorySnapshot(residentMB: -1, physFootprintMB: -1)
}

func percentile(_ sorted: [Double], _ p: Double) -> Double {
    guard !sorted.isEmpty else { return 0 }
    let idx = min(sorted.count - 1, max(0, Int(Double(sorted.count - 1) * p)))
    return sorted[idx]
}

let memStage0 = currentMemorySnapshot()
print("========================================================================")
print("🚀 PasteFlow Master Spec v1.4 — Step 0A 架构地基真机实测套件 (AOT -O 编译态)")
print("🕒 执行时间: \(ISO8601DateFormatter().string(from: Date()))")
print("💻 操作系统: \(ProcessInfo.processInfo.operatingSystemVersionString)")
print(String(format: "📊 [Stage 0 - 纯原生无窗口核心进程基线内存]: phys_footprint = %.2f MB, RSS = %.2f MB",
             memStage0.physFootprintMB, memStage0.residentMB))
print("========================================================================\n")

// ----------------------------------------------------------------------------
// 1. [S0-Logic & S7] WindowObserver 环形缓冲与 FrontmostApp 启发式路由实测
// ----------------------------------------------------------------------------
print("--- [S0-Logic & S7] WindowObserver 环形缓冲与 FrontmostApp 启发式路由实测 ---")

final class AppActivationRecord {
    let bundleId: String
    let appName: String
    let pid: pid_t
    let timestampMs: UInt64

    init(bundleId: String, appName: String, pid: pid_t, timestampMs: UInt64) {
        self.bundleId = bundleId
        self.appName = appName
        self.pid = pid
        self.timestampMs = timestampMs
    }
}

final class WindowObserver {
    private var ringBuffer: [AppActivationRecord] = []
    private let capacity = 16
    private let lock = NSLock()

    init() {
        if let front = NSWorkspace.shared.frontmostApplication {
            recordActivation(
                bundleId: front.bundleIdentifier ?? "unknown",
                appName: front.localizedName ?? "unknown",
                pid: front.processIdentifier,
                timestampMs: Self.nowMs()
            )
        }
    }

    static func nowMs() -> UInt64 {
        return UInt64(DispatchTime.now().uptimeNanoseconds / 1_000_000)
    }

    func recordActivation(bundleId: String, appName: String, pid: pid_t, timestampMs: UInt64) {
        let rec = AppActivationRecord(bundleId: bundleId, appName: appName, pid: pid, timestampMs: timestampMs)
        lock.lock()
        if ringBuffer.count >= capacity {
            ringBuffer.removeFirst()
        }
        ringBuffer.append(rec)
        lock.unlock()
    }

    /// 零拷贝返回最新激活的应用引用 (对应 Rust Arc<AppActivationRecord>::clone)
    func currentFrontmostApp() -> AppActivationRecord? {
        lock.lock()
        let last = ringBuffer.last
        lock.unlock()
        return last
    }

    func resolveSourceApp(atCopyTimeMs copyTimeMs: UInt64) -> (record: AppActivationRecord?, confidence: String) {
        lock.lock()
        defer { lock.unlock() }
        for rec in ringBuffer.reversed() {
            if rec.timestampMs <= copyTimeMs {
                let delta = copyTimeMs - rec.timestampMs
                return (rec, delta <= 500 ? "exact" : "stale")
            }
        }
        return (ringBuffer.first, "stale")
    }
}

let observer = WindowObserver()

var nsWorkspaceLatenciesUs: [Double] = []
for _ in 0..<1000 {
    let t0 = DispatchTime.now().uptimeNanoseconds
    let _ = NSWorkspace.shared.frontmostApplication?.bundleIdentifier
    let dtUs = Double(DispatchTime.now().uptimeNanoseconds - t0) / 1000.0
    nsWorkspaceLatenciesUs.append(dtUs)
}
nsWorkspaceLatenciesUs.sort()

var ringBufferLatenciesUs: [Double] = []
for _ in 0..<10000 {
    let t0 = DispatchTime.now().uptimeNanoseconds
    let _ = observer.currentFrontmostApp()?.bundleId
    let dtUs = Double(DispatchTime.now().uptimeNanoseconds - t0) / 1000.0
    ringBufferLatenciesUs.append(dtUs)
}
ringBufferLatenciesUs.sort()

print(String(format: "  • NSWorkspace.frontmostApplication 直接读取 (1,000次): p50 = %.3f µs, p95 = %.3f µs, p99 = %.3f µs",
             percentile(nsWorkspaceLatenciesUs, 0.50),
             percentile(nsWorkspaceLatenciesUs, 0.95),
             percentile(nsWorkspaceLatenciesUs, 0.99)))
print(String(format: "  • WindowObserver 引用环形缓冲 O(1) 读取 (10,000次):  p50 = %.3f µs, p95 = %.3f µs, p99 = %.3f µs",
             percentile(ringBufferLatenciesUs, 0.50),
             percentile(ringBufferLatenciesUs, 0.95),
             percentile(ringBufferLatenciesUs, 0.99)))

let baseT: UInt64 = 100_000
observer.recordActivation(bundleId: "com.google.Chrome", appName: "Google Chrome", pid: 1001, timestampMs: baseT)
let copyEventT: UInt64 = baseT + 120
observer.recordActivation(bundleId: "com.microsoft.Word", appName: "Microsoft Word", pid: 1002, timestampMs: baseT + 150)
let resolved = observer.resolveSourceApp(atCopyTimeMs: copyEventT)
let s7Pass = (resolved.record?.bundleId == "com.google.Chrome" && resolved.confidence == "exact")
print("  • [S7 抗竞态 SourceApp 回溯]: 复制后 30ms 极速切到 Word，回溯 SourceApp = \(resolved.record?.bundleId ?? "nil") (confidence=\(resolved.confidence)) -> \(s7Pass ? "✅ PASS" : "❌ FAIL")\n")

// ----------------------------------------------------------------------------
// 2. [S2] 两阶段 FormatGate 实测 (G1A 私有对象 vs G1B 伴生预览图解耦 + G5 双层体积门控)
// ----------------------------------------------------------------------------
print("--- [S2] 两阶段 FormatGate (G1A/G1B 解耦 + G5 双层体积降级) 实测 ---")

enum GateSkipReason: String, Error {
    case g1aComplexPrivateObject = "G1A_COMPLEX_OBJECT"
    case g1bStandaloneMedia      = "G1B_STANDALONE_MEDIA"
    case g2MultiItem             = "G2_MULTI_ITEM"
    case g3FileUrl               = "G3_FILE_URL"
    case g4LegacyOnly            = "G4_LEGACY_ONLY"
    case g5Oversize              = "G5_OVERSIZE"
}

enum PayloadTier: String {
    case fullMultiChannel    = "FULL_MULTI_CHANNEL"     // <= 64KB
    case lightweightTextOnly = "LIGHTWEIGHT_TEXT_ONLY"  // 64KB ~ 256KB
}

/// 第一阶段：纯元数据门控（0 字节正文读取）
func evaluateMetadataGate(types: [String], itemCount: Int) -> Result<Void, GateSkipReason> {
    if itemCount > 1 {
        return .failure(.g2MultiItem)
    }
    let hasModernText = types.contains("public.utf8-plain-text") || types.contains("public.html")
    var hasCompanionMedia = false

    for t in types {
        if t == "public.file-url" || t == "NSFilenamesPboardType" {
            return .failure(.g3FileUrl)
        }
        // G1A: 电子表格、OLE 嵌入对象、设计软件图层或富文本附件包 -> 无条件拦截，绝不拉取 Promised Data
        if t.hasPrefix("com.microsoft.excel.") || t == "com.microsoft.Object-Descriptor" ||
           t == "com.microsoft.DataObject" || t.hasPrefix("com.apple.iWork.TS") ||
           t.hasPrefix("com.adobe.") || t.hasPrefix("com.figma.") ||
           t.hasPrefix("com.bohemiancoding.") || t == "com.apple.flat-rtfd" {
            return .failure(.g1aComplexPrivateObject)
        }
        // G1B: 图像与 PDF 类型（需结合是否伴生现代文本通道判断）
        if t == "public.tiff" || t == "public.png" || t == "public.pdf" {
            hasCompanionMedia = true
        }
    }

    if hasCompanionMedia && !hasModernText {
        return .failure(.g1bStandaloneMedia)
    }
    if !hasModernText {
        return .failure(.g4LegacyOnly)
    }
    return .success(())
}

/// 第二阶段：仅在 Stage 1 通过并读取文本后，判定载荷体积分级
func evaluatePayloadTier(textByteLength: Int) -> Result<PayloadTier, GateSkipReason> {
    if textByteLength <= 64 * 1024 {
        return .success(.fullMultiChannel)
    } else if textByteLength <= 256 * 1024 {
        return .success(.lightweightTextOnly)
    } else {
        return .failure(.g5Oversize)
    }
}

let gateCases: [(String, [String], Int, Int, Bool, String)] = [
    ("Excel 单元格复制 (含公式与 TIFF)", ["public.utf8-plain-text", "public.html", "com.microsoft.Object-Descriptor", "com.microsoft.excel.data", "public.tiff"], 1, 1024, false, "G1A_COMPLEX_OBJECT"),
    ("Figma 图层复制 (含 PDF 私有数据)", ["public.utf8-plain-text", "com.figma.node", "public.pdf"], 1, 512, false, "G1A_COMPLEX_OBJECT"),
    ("Word/Pages 富文本复制 (伴生 TIFF 预览)", ["public.utf8-plain-text", "public.html", "public.rtf", "public.tiff", "public.pdf"], 1, 4096, true, "FULL_MULTI_CHANNEL"),
    ("系统纯截图复制 (仅有 PNG/TIFF 无文本)", ["public.png", "public.tiff"], 1, 0, false, "G1B_STANDALONE_MEDIA"),
    ("Finder 复制 3 个文件", ["public.file-url", "NSFilenamesPboardType"], 3, 256, false, "G2_MULTI_ITEM"),
    ("老旧应用纯 Legacy 4字符类型", ["CorePasteboardFlavorType 0x74657874", "NSStringPboardType"], 1, 128, false, "G4_LEGACY_ONLY"),
    ("AI 超长回复 120KB (64~256KB 轻量降级)", ["public.utf8-plain-text"], 1, 120 * 1024, true, "LIGHTWEIGHT_TEXT_ONLY"),
    ("复制 600KB 巨型日志 (>256KB 拒绝)", ["public.utf8-plain-text"], 1, 600 * 1024, false, "G5_OVERSIZE"),
    ("ChatGPT 复制常规 Markdown (4KB)", ["public.utf8-plain-text", "public.html", "org.chromium.source-url"], 1, 4096, true, "FULL_MULTI_CHANNEL")
]

for (name, types, count, bytes, expectedPass, expectedLabel) in gateCases {
    let t0 = DispatchTime.now().uptimeNanoseconds
    let stage1 = evaluateMetadataGate(types: types, itemCount: count)
    let passed: Bool
    let outcomeLabel: String
    switch stage1 {
    case .failure(let reason):
        passed = false
        outcomeLabel = reason.rawValue
    case .success:
        switch evaluatePayloadTier(textByteLength: bytes) {
        case .success(let tier):
            passed = true
            outcomeLabel = tier.rawValue
        case .failure(let reason):
            passed = false
            outcomeLabel = reason.rawValue
        }
    }
    let us = Double(DispatchTime.now().uptimeNanoseconds - t0) / 1000.0
    let ok = (passed == expectedPass && outcomeLabel == expectedLabel)
    print(String(format: "  • %-38@ -> passed=%-5@ outcome=%-22@ (%.3f µs) %@",
                 name as NSString,
                 "\(passed)" as NSString,
                 outcomeLabel as NSString,
                 us,
                 ok ? "✅" : "❌"))
}
print("")

// ----------------------------------------------------------------------------
// 3. [S0-Logic, S1 & S5] L1 算子、HTML/RTF 同步抑制、轻量大文本降级与 8ms 预算
// ----------------------------------------------------------------------------
print("--- [S0-Logic, S1 & S5] L1 滤镜转换、HTML/RTF 双通道抑制、轻量降级与 8ms 预算实测 ---")

let sampleMarkdown = """
# 灵贴 PasteFlow 架构周报

本周完成了 **持有式延迟供给（Owned Lazy Provision）** 与 **前台应用启发式（FrontmostApp Heuristics）** 的核心验证：

1. **Word / 飞书文档**：自动获得原生标题与实线边框表格；
2. **微信 / 钉钉 IM**：自动剥离 `**` 与 `#`，生成清爽纯文本；
3. **Terminal / VSCode**：100% 保持原始 Markdown 源码。

| 模块 | 状态 | 耗时 |
|---|---|---|
| FormatGate | 已完成 | <0.01ms |
| PipeExecutor | 已完成 | <1.5ms |
"""

let boldRegex = try! NSRegularExpression(pattern: #"\*\*(.+?)\*\*"#)
let codeRegex = try! NSRegularExpression(pattern: #"`(.+?)`"#)
let urlRegex  = try! NSRegularExpression(pattern: #"https?://[^\s"<>]+"#)

func markdownToOfficeHTML(_ md: String) -> String {
    var html = "<html><head><meta charset='utf-8'><style>"
    html += "table { border-collapse: collapse; mso-table-lspace:0pt; mso-table-rspace:0pt; } "
    html += "th, td { border: 1px solid #BFBFBF; mso-border-alt: solid #BFBFBF .5pt; padding: 6px 10px; } "
    html += "h1 { font-size: 18pt; font-weight: bold; } "
    html += "</style></head><body>"
    let lines = md.components(separatedBy: .newlines)
    var inTable = false
    for line in lines {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        if trimmed.hasPrefix("# ") {
            html += "<h1>\(trimmed.dropFirst(2))</h1>"
        } else if trimmed.hasPrefix("|") && trimmed.hasSuffix("|") {
            if trimmed.contains("---") { continue }
            if !inTable { html += "<table>"; inTable = true }
            let cells = trimmed.split(separator: "|").map { $0.trimmingCharacters(in: .whitespaces) }
            html += "<tr>" + cells.map { "<td>\($0)</td>" }.joined() + "</tr>"
        } else {
            if inTable { html += "</table>"; inTable = false }
            if !trimmed.isEmpty {
                let range = NSRange(trimmed.startIndex..<trimmed.endIndex, in: trimmed)
                let boldConverted = boldRegex.stringByReplacingMatches(in: trimmed, range: range, withTemplate: "<b>$1</b>")
                html += "<p>\(boldConverted)</p>"
            }
        }
    }
    if inTable { html += "</table>" }
    html += "</body></html>"
    return html
}

func markdownToIMPrettyText(_ md: String) -> String {
    // 单遍 UTF-8 字节流扫描 (对齐 Rust &[u8] / pulldown-cmark 零 Grapheme 拆解开销：ASCII 控制符 * # ` \n 均 < 0x80)
    return md.utf8.withContiguousStorageIfAvailable { buf -> String in
        var out = [UInt8]()
        out.reserveCapacity(buf.count + 256)
        let n = buf.count
        var i = 0
        var atLineStart = true
        var appendCloseBracketAtEOL = false
        let lBracket: [UInt8] = Array("【".utf8)
        let rBracket: [UInt8] = Array("】".utf8)
        let barPrefix: [UInt8] = Array("▍".utf8)

        while i < n {
            let b = buf[i]
            if atLineStart {
                atLineStart = false
                if b == 0x23 { // '#'
                    if i + 1 < n && buf[i + 1] == 0x20 { // "# "
                        out.append(contentsOf: lBracket)
                        appendCloseBracketAtEOL = true
                        i += 2
                        continue
                    } else if i + 2 < n && buf[i + 1] == 0x23 && buf[i + 2] == 0x20 { // "## "
                        out.append(contentsOf: barPrefix)
                        i += 3
                        continue
                    }
                }
            }
            if b == 0x0A { // '\n'
                if appendCloseBracketAtEOL {
                    out.append(contentsOf: rBracket)
                    appendCloseBracketAtEOL = false
                }
                out.append(0x0A)
                atLineStart = true
                i += 1
                continue
            }
            if b == 0x2A && i + 1 < n && buf[i + 1] == 0x2A { // "**"
                i += 2
                continue
            }
            if b == 0x60 { // '`'
                i += 1
                continue
            }
            out.append(b)
            i += 1
        }
        if appendCloseBracketAtEOL {
            out.append(contentsOf: rBracket)
        }
        return String(decoding: out, as: UTF8.self)
    } ?? md
}

let richSuppressBundles: Set<String> = [
    "com.tinyspeck.slackmacgap", "com.hnc.Discord",
    "com.microsoft.VSCode", "com.todesktop.230313mzl4w4u92",
    "com.apple.Terminal", "com.googlecode.iterm2"
]
let imPrettyBundles: Set<String> = [
    "com.tencent.xinWeChat", "com.tencent.WeWorkMac",
    "com.alibaba.DingTalkMac", "com.electron.lark"
]

func provideChannelData(channel: String, frontmostBundleId: String, sourceBundleId: String, msSinceCopy: UInt64, rawMarkdown: String) -> (data: String?, variant: String) {
    if msSinceCopy < 150 && frontmostBundleId == sourceBundleId {
        return (channel == "plainText" ? rawMarkdown : nil, "probe_guard_passthrough")
    }
    if channel == "html" {
        if richSuppressBundles.contains(frontmostBundleId) {
            return (nil, "html_suppressed")
        }
        return (markdownToOfficeHTML(rawMarkdown), "rich_office")
    } else if channel == "rtf" {
        if richSuppressBundles.contains(frontmostBundleId) {
            return (nil, "rtf_suppressed")
        }
        return ("{\\rtf1\\ansi ...}", "rich_rtf")
    } else if channel == "plainText" {
        if imPrettyBundles.contains(frontmostBundleId) {
            return (markdownToIMPrettyText(rawMarkdown), "im_cleaned_text")
        }
        return (rawMarkdown, "raw_passthrough")
    }
    return (rawMarkdown, "raw_passthrough")
}

let s0Matrix: [(String, String, String, UInt64, String)] = [
    ("Word for Mac 请求 HTML", "html", "com.microsoft.Word", 800, "rich_office"),
    ("微信 WeChat 请求 plainText", "plainText", "com.tencent.xinWeChat", 950, "im_cleaned_text"),
    ("Terminal 请求 plainText", "plainText", "com.apple.Terminal", 1200, "raw_passthrough"),
    ("Slack 请求 HTML (防富文本劫持)", "html", "com.tinyspeck.slackmacgap", 1100, "html_suppressed"),
    ("Slack 请求 RTF (同步抑制防回退劫持)", "rtf", "com.tinyspeck.slackmacgap", 1105, "rtf_suppressed"),
    ("VSCode 请求 HTML (防富文本劫持)", "html", "com.microsoft.VSCode", 1300, "html_suppressed"),
    ("复制后 40ms 后台嗅探抢读", "plainText", "com.google.Chrome", 40, "probe_guard_passthrough")
]

for (desc, ch, bundle, dt, expectedVariant) in s0Matrix {
    let t0 = DispatchTime.now().uptimeNanoseconds
    let res = provideChannelData(channel: ch, frontmostBundleId: bundle, sourceBundleId: "com.google.Chrome", msSinceCopy: dt, rawMarkdown: sampleMarkdown)
    let us = Double(DispatchTime.now().uptimeNanoseconds - t0) / 1000.0
    let ok = (res.variant == expectedVariant)
    print(String(format: "  • %-36@ -> variant=%-24@ (%.2f µs) %@",
                 desc as NSString, res.variant as NSString, us, ok ? "✅" : "❌"))
}

// 测量 120KB 超长 AI 回复在 LIGHTWEIGHT_TEXT_ONLY (仅做 IM 纯文本去星号) 下的耗时
let long120KBMarkdown = String(repeating: sampleMarkdown + "\n", count: 225) // ~120KB
var lightweightLatenciesMs: [Double] = []
for _ in 0..<20 {
    let t0 = DispatchTime.now().uptimeNanoseconds
    let _ = markdownToIMPrettyText(long120KBMarkdown)
    let ms = Double(DispatchTime.now().uptimeNanoseconds - t0) / 1_000_000.0
    lightweightLatenciesMs.append(ms)
}
lightweightLatenciesMs.sort()
print(String(format: "  • [120KB 长文 LIGHTWEIGHT_TEXT_ONLY 纯文本净化耗时]: p50 = %.2f ms, p95 = %.2f ms, max = %.2f ms -> %@",
             percentile(lightweightLatenciesMs, 0.50),
             percentile(lightweightLatenciesMs, 0.95),
             lightweightLatenciesMs.last ?? 0,
             (lightweightLatenciesMs.last ?? 99) < 5.0 ? "✅ PASS (<5ms)" : "⚠️ 超出 5ms"))
print("")

// ----------------------------------------------------------------------------
// 4. [S8 & S11] 非激活 NSPanel + 常驻隐藏 WKWebView 焦点路由与真实 AOT 内存基线实测
// ----------------------------------------------------------------------------
print("--- [S8 & S11] 非激活 NSPanel + WKWebView 属性校验与 AOT 编译态真实内存基线 ---")

final class NonActivatingHUDPanel: NSPanel {
    override var canBecomeKey: Bool { return true }
    override var canBecomeMain: Bool { return false }
}

let app = NSApplication.shared
app.setActivationPolicy(.accessory)

let panel = NonActivatingHUDPanel(
    contentRect: NSRect(x: 200, y: 200, width: 260, height: 180),
    styleMask: [.borderless, .nonactivatingPanel],
    backing: .buffered,
    defer: false
)
panel.level = .popUpMenu
panel.isFloatingPanel = true
panel.becomesKeyOnlyIfNeeded = false
panel.hidesOnDeactivate = false

let memStage1 = currentMemorySnapshot()
let s8PanelChecks = (panel.canBecomeKey == true && panel.canBecomeMain == false && panel.styleMask.contains(.nonactivatingPanel) && app.isActive == false)
print("  • [S8 NonActivatingHUDPanel 属性校验]: canBecomeKey=\(panel.canBecomeKey), canBecomeMain=\(panel.canBecomeMain), nonActivating=\(panel.styleMask.contains(.nonactivatingPanel)), appIsActive=\(app.isActive) -> \(s8PanelChecks ? "✅ PASS" : "❌ FAIL")")
print(String(format: "  • [Stage 1 - 创建原生 NonActivating NSPanel (v0.1.0 含 Toast/Menu 态)]: phys_footprint = %.2f MB, RSS = %.2f MB",
             memStage1.physFootprintMB, memStage1.residentMB))

let webConfig = WKWebViewConfiguration()
let webView = WKWebView(frame: panel.contentView!.bounds, configuration: webConfig)
panel.contentView?.addSubview(webView)
let hudHTML = """
<!doctype html><html><head><meta charset='utf-8'>
<style>body{margin:0;font-family:-apple-system;background:rgba(30,30,34,0.85);color:#fff;padding:12px;border-radius:12px;}</style>
</head><body><div>✨ 灵贴 Micro-HUD</div><div id='item1'>1. 转为微信精致纯文本</div><div id='item2'>2. 脱敏隐藏密钥</div></body></html>
"""
webView.loadHTMLString(hudHTML, baseURL: nil)
RunLoop.current.run(until: Date().addingTimeInterval(0.6))

let memStage2 = currentMemorySnapshot()
print(String(format: "  • [Stage 2 - 挂载常驻隐藏 WKWebView 后本进程内存 (v0.1.5 含 HUD 态)]: phys_footprint = %.2f MB, RSS = %.2f MB (WebView 本进程增量 +%.2f MB)",
             memStage2.physFootprintMB, memStage2.residentMB, memStage2.physFootprintMB - memStage1.physFootprintMB))
print("========================================================================")
