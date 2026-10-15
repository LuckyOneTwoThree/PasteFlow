import Foundation
import AppKit
import WebKit

// ============================================================================
// PasteFlow (灵贴) — Step 0B 跨进程 E2E 冒烟准入套件 (Dev Plan P0)
// 覆盖验证项:
//   • S0-E2E: 跨进程 DataProvider 延迟供给 + FrontmostApp 启发式路由
//             + HTML/RTF 双通道同步抑制 (nil 回退 plainText 协商)
//             + 150ms 后台抢读 Probe Guard
//   • S8-E2E: 非激活 NSPanel + WKWebView 真实按键吞吐 (物理按键需人工配合)
//
// 用法:
//   step0b_e2e serve <logfile>                 挂载 Provider 并记录供给日志
//   step0b_e2e probe <bundleId>...             逐个激活目标应用并执行通道协商读取
//   step0b_e2e hud <logfile>                   弹出非激活 HUD 并等待物理按键 (1/Esc)
// ============================================================================

let SAMPLE_MARKDOWN = """
# 灵贴冒烟标题
**加粗文本** 与 `行内代码`
- 列表项 A
- 列表项 B
| 列1 | 列2 |
|---|---|
| a | b |
"""

// IM 纯文本净化白名单 (Master Spec §3.2)
let IM_TARGET_BUNDLES: Set<String> = [
    "com.tencent.xinWeChat",
    "com.tencent.WeWorkMac",
    "com.alibaba.DingTalkMac",
    "com.electron.lark",
]

// 富文本注入抑制名单 (Master Spec §3.2)
let SUPPRESS_BUNDLES: Set<String> = [
    "com.tinyspeck.slackmacgap",
    "com.hnc.Discord",
    "com.microsoft.VSCode",
    "com.todesktop.230313mzl4w4u92", // Cursor
    "com.apple.Terminal",
    "com.googlecode.iterm2",
]

func logLine(_ file: String?, _ line: String) {
    print(line)
    if let file = file {
        if let fh = FileHandle(forWritingAtPath: file) {
            fh.seekToEndOfFile()
            fh.write((line + "\n").data(using: .utf8)!)
            try? fh.close()
        } else {
            try? (line + "\n").data(using: .utf8)!.write(to: URL(fileURLWithPath: file))
        }
    }
}

// ----------------------------------------------------------------------------
// S0-E2E: Provider 模式
// ----------------------------------------------------------------------------
final class LoggingProvider: NSObject, NSPasteboardItemDataProvider {
    let logFile: String
    let copyTsMs: UInt64
    let sourceBundle: String

    init(logFile: String, copyTsMs: UInt64, sourceBundle: String) {
        self.logFile = logFile
        self.copyTsMs = copyTsMs
        self.sourceBundle = sourceBundle
    }

    func pasteboard(_ pasteboard: NSPasteboard?, item: NSPasteboardItem, provideDataForType type: NSPasteboard.PasteboardType) {
        let nowMs = UInt64(Date().timeIntervalSince1970 * 1000)
        let elapsed = nowMs &- copyTsMs
        let front = NSWorkspace.shared.frontmostApplication
        let frontBundle = front?.bundleIdentifier ?? "unknown"
        let frontName = front?.localizedName ?? "?"

        // 变体判定 (PipeExecutor 决策树, TDD §3.4)
        let variant: String
        let payload: String?
        let probeGuard = elapsed < 150 && frontBundle == sourceBundle

        switch type.rawValue {
        case "public.html":
            if probeGuard { variant = "probe_guard_passthrough"; payload = "<p>raw-html</p>" }
            else if SUPPRESS_BUNDLES.contains(frontBundle) { variant = "html_suppressed"; payload = nil }
            else { variant = "rich_office"; payload = "<h1>灵贴冒烟标题</h1><p><b>加粗文本</b></p>" }
        case "public.rtf":
            if probeGuard { variant = "probe_guard_passthrough"; payload = "{\\rtf1 raw}" }
            else if SUPPRESS_BUNDLES.contains(frontBundle) { variant = "rtf_suppressed"; payload = nil }
            else { variant = "rich_office"; payload = "{\\rtf1\\ansi rich}" }
        case "public.utf8-plain-text":
            if probeGuard { variant = "probe_guard_passthrough"; payload = SAMPLE_MARKDOWN }
            else if IM_TARGET_BUNDLES.contains(frontBundle) { variant = "im_cleaned_text"; payload = "【灵贴冒烟标题】\n加粗文本 与 行内代码\n • 列表项 A\n • 列表项 B" }
            else { variant = "raw_passthrough"; payload = SAMPLE_MARKDOWN }
        default:
            variant = "raw_passthrough"; payload = nil
        }

        let msg = "[SERVED] t=+\(elapsed)ms channel=\(type.rawValue) frontmost=\(frontName)(\(frontBundle)) variant=\(variant) payloadLen=\(payload?.utf8.count ?? 0)"
        logLine(logFile, msg)

        if let p = payload {
            item.setString(p, forType: type)
        }
        // payload == nil 时故意不写入数据: 验证消费方回退链
    }

    func pasteboardFinishedWithDataProvider(_ pasteboard: NSPasteboard) {
        logLine(logFile, "[PROVIDER] pasteboardFinishedWithDataProvider")
    }
}

/// 决定性实验 exp4: provider 在第 1 次回调时【拒供】(不写入数据),
/// 验证 pasteboard 服务器是否会因未供给而在后续读取时【重新回调】。
///   - 若重新回调 -> 首读冻结仅是"首写冻结", 可通过"拒供嗅探者"救活延迟供给模型
///   - 若不再回调 -> "首次读取即冻结"(含空值), 延迟供给模型必须重构
final class RefusingProvider: NSObject, NSPasteboardItemDataProvider {
    let logFile: String
    var callCounts: [String: Int] = [:]

    init(logFile: String) { self.logFile = logFile }

    func pasteboard(_ pasteboard: NSPasteboard?, item: NSPasteboardItem, provideDataForType type: NSPasteboard.PasteboardType) {
        let n = (callCounts[type.rawValue] ?? 0) + 1
        callCounts[type.rawValue] = n
        let front = NSWorkspace.shared.frontmostApplication
        let frontBundle = front?.bundleIdentifier ?? "unknown"
        logLine(logFile, "[CALL#\(n)] channel=\(type.rawValue) frontmost=\(frontBundle)")
        if n >= 2 {
            item.setString("PROVIDED_ON_CALL_\(n)", forType: type)
            logLine(logFile, "[CALL#\(n)] -> 已供给数据")
        } else {
            logLine(logFile, "[CALL#\(n)] -> 拒供(未写入数据), 观察是否重新回调")
        }
    }

    func pasteboardFinishedWithDataProvider(_ pasteboard: NSPasteboard) {
        logLine(logFile, "[PROVIDER] pasteboardFinishedWithDataProvider")
    }
}

func runServeRefuse(logFile: String) {
    try? "".data(using: .utf8)!.write(to: URL(fileURLWithPath: logFile))
    let pb = NSPasteboard.general
    pb.clearContents()
    let item = NSPasteboardItem()
    let provider = RefusingProvider(logFile: logFile)
    item.setDataProvider(provider, forTypes: [
        .init("public.html"),
        .init("public.rtf"),
        .init("public.utf8-plain-text"),
    ])
    pb.writeObjects([item])
    logLine(logFile, "[OWNED-REFUSE] changeCount=\(pb.changeCount)")
    print("RefusingProvider owned. Waiting...")
    RunLoop.main.run()
}

/// 决定性实验 exp5: Re-own 补挂 —— 首次回调(嗅探者抢读)后自动重新 clearContents+writeObjects
/// 挂载新一代 provider, 验证真实消费方读取能否触发【新一代】回调(即救活延迟供给)。
///   - 若新一代被回调 -> "嗅探者抢读后 Re-own 补挂" 可行, 作为 DYNAMIC_BY_TARGET 的救活方案
///   - 若不被回调     -> 补挂无效, 必须改为静态多通道或复制时决策
final class ReownProvider: NSObject, NSPasteboardItemDataProvider {
    let logFile: String
    let generation: Int
    let onFirstServe: () -> Void
    var served = false

    init(logFile: String, generation: Int, onFirstServe: @escaping () -> Void) {
        self.logFile = logFile
        self.generation = generation
        self.onFirstServe = onFirstServe
    }

    func pasteboard(_ pasteboard: NSPasteboard?, item: NSPasteboardItem, provideDataForType type: NSPasteboard.PasteboardType) {
        let front = NSWorkspace.shared.frontmostApplication?.bundleIdentifier ?? "unknown"
        logLine(logFile, "[G\(generation)] channel=\(type.rawValue) frontmost=\(front)")
        item.setString("GEN\(generation)_\(type.rawValue)", forType: type)
        if !served {
            served = true
            DispatchQueue.main.async { self.onFirstServe() }
        }
    }

    func pasteboardFinishedWithDataProvider(_ pasteboard: NSPasteboard) {
        logLine(logFile, "[G\(generation)] finishedWithDataProvider")
    }
}

func runServeReown(logFile: String, reownDelayMs: Int) {
    try? "".data(using: .utf8)!.write(to: URL(fileURLWithPath: logFile))
    let pb = NSPasteboard.general

    func mount(generation: Int) {
        pb.clearContents()
        let item = NSPasteboardItem()
        let provider = ReownProvider(logFile: logFile, generation: generation) {
            logLine(logFile, "[G\(generation)] 首次供给已发生, \(reownDelayMs)ms 后 Re-own 补挂...")
            DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(reownDelayMs)) {
                logLine(logFile, "[REOWN] 开始补挂 G\(generation + 1), changeCount=\(pb.changeCount)")
                mount(generation: generation + 1)
            }
        }
        item.setDataProvider(provider, forTypes: [
            .init("public.html"),
            .init("public.rtf"),
            .init("public.utf8-plain-text"),
        ])
        pb.writeObjects([item])
        logLine(logFile, "[OWNED-G\(generation)] changeCount=\(pb.changeCount)")
    }

    mount(generation: 1)
    print("ReownProvider owned (G1). Waiting...")
    RunLoop.main.run()
}

func runServe(logFile: String, guardRead: Bool = true) {
    try? "".data(using: .utf8)!.write(to: URL(fileURLWithPath: logFile))
    let pb = NSPasteboard.general
    pb.clearContents()
    let item = NSPasteboardItem()
    // Probe Guard 语义: SourceApp 取挂载瞬间的前台应用, 使其与 <150ms 内抢读时的前台一致
    let sourceBundle = NSWorkspace.shared.frontmostApplication?.bundleIdentifier ?? "com.pasteflow.spike"
    let provider = LoggingProvider(
        logFile: logFile,
        copyTsMs: UInt64(Date().timeIntervalSince1970 * 1000),
        sourceBundle: sourceBundle
    )
    item.setDataProvider(provider, forTypes: [
        .init("public.html"),
        .init("public.rtf"),
        .init("public.utf8-plain-text"),
    ])
    pb.writeObjects([item])
    logLine(logFile, "[OWNED] changeCount=\(pb.changeCount) sourceBundle=\(sourceBundle) declaredTypes=\(item.types.map { $0.rawValue })")

    // 内部 50ms 抢读: 验证 Probe Guard (elapsed < 150ms && frontmost == sourceBundle)
    if guardRead {
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) {
            _ = pb.string(forType: .init("public.utf8-plain-text"))
        }
    }
    print("Provider owned. Waiting for consumers... (Ctrl+C to exit)")
    RunLoop.main.run()
}

// ----------------------------------------------------------------------------
// S0-E2E: Probe 消费模式 (模拟目标应用的通道协商)
// ----------------------------------------------------------------------------
func activateApp(bundleId: String) -> Bool {
    guard let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleId) else { return false }
    let conf = NSWorkspace.OpenConfiguration()
    conf.activates = true
    NSWorkspace.shared.openApplication(at: url, configuration: conf) { _, _ in }
    return true
}

/// 决定性实验: 仅读取 plainText 通道, 验证 macOS 是否对同一 type 多次读取重复回调 provider
/// (DYNAMIC_BY_TARGET 依赖"每次回调按 FrontmostApp 实时分支", 若固化则架构需修订)
func runProbePlain(bundleIds: [String]) {
    let pb = NSPasteboard.general
    for bid in bundleIds {
        guard let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bid) else {
            print("[PROBE-PLAIN] \(bid) -> NOT INSTALLED, SKIP")
            continue
        }
        let name = FileManager.default.displayName(atPath: url.path)
        _ = activateApp(bundleId: bid)
        Thread.sleep(forTimeInterval: 0.8)
        let val = pb.string(forType: .init("public.utf8-plain-text"))
        let desc = val.map { "len=\($0.utf8.count) head=\($0.prefix(20).replacingOccurrences(of: "\n", with: "⏎"))" } ?? "nil"
        print("[PROBE-PLAIN] \(name) (\(bid)) plainText=\(desc)  (对照 serve 日志判断是否再次回调)")
    }
}

func runProbe(bundleIds: [String], probeGuardTest: Bool) {
    let pb = NSPasteboard.general
    let sem = DispatchSemaphore(value: 0)

    if probeGuardTest {
        // 快速抢读测试: Provider 刚持有 (<150ms) 且前台仍为源应用语义
        let t0 = Date()
        _ = pb.string(forType: .init("public.utf8-plain-text"))
        let ms = Date().timeIntervalSince(t0) * 1000
        print("[PROBE-GUARD] immediate read elapsed=\(String(format: "%.3f", ms))ms (期望命中 probe_guard_passthrough)")
        return
    }

    for bid in bundleIds {
        guard let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bid) else {
            print("[PROBE] \(bid) -> NOT INSTALLED, SKIP")
            continue
        }
        let name = FileManager.default.displayName(atPath: url.path)
        let ok = activateApp(bundleId: bid)
        print("\n[PROBE] === \(name) (\(bid)) activated=\(ok) ===")
        Thread.sleep(forTimeInterval: 0.8)

        // 模拟 Slack/Discord/Electron 协商顺序: html -> rtf -> plainText
        var htmlVal = pb.string(forType: .init("public.html"))
        Thread.sleep(forTimeInterval: 0.15)
        var rtfVal = pb.string(forType: .init("public.rtf"))
        Thread.sleep(forTimeInterval: 0.15)
        var plainVal = pb.string(forType: .init("public.utf8-plain-text"))
        Thread.sleep(forTimeInterval: 0.15)

        let htmlDesc = htmlVal.map { "len=\($0.utf8.count)" } ?? "nil"
        let rtfDesc = rtfVal.map { "len=\($0.utf8.count)" } ?? "nil"
        let plainDesc = plainVal.map { "len=\($0.utf8.count) head=\($0.prefix(24).replacingOccurrences(of: "\n", with: "⏎"))" } ?? "nil"
        print("[PROBE] html=\(htmlDesc) rtf=\(rtfDesc) plainText=\(plainDesc)")

        // 回退链判定 (R-1 核心)
        if htmlVal == nil && rtfVal == nil && plainVal != nil {
            print("[PROBE] ✅ PASS: html/rtf 同步抑制 -> 回退链命中 plainText (raw)")
        } else if htmlVal != nil && plainVal != nil {
            print("[PROBE] ✅ PASS: 富文本通道供给 + plainText 伴随")
        }
        htmlVal = nil; rtfVal = nil; plainVal = nil
    }
    sem.signal()
    _ = sem.wait(timeout: .now() + 0.1)
}

// ----------------------------------------------------------------------------
// S8-E2E: 非激活 NSPanel + WKWebView 按键吞吐
// ----------------------------------------------------------------------------
final class NonActivatingHUDPanel: NSPanel {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }
}

final class KeyBridge: NSObject, WKScriptMessageHandler {
    let logFile: String
    var onDone: (() -> Void)?
    init(logFile: String) { self.logFile = logFile }
    func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
        logLine(logFile, "[HUD-KEY] \(message.body)")
        if "\(message.body)".contains("Escape") {
            DispatchQueue.main.async { self.onDone?() }
        }
    }
}

func runHud(logFile: String) {
    try? "".data(using: .utf8)!.write(to: URL(fileURLWithPath: logFile))
    let app = NSApplication.shared
    app.setActivationPolicy(.accessory) // 不占 Dock, 模拟菜单栏 App

    let panel = NonActivatingHUDPanel(
        contentRect: NSRect(x: 300, y: 300, width: 320, height: 180),
        styleMask: [.borderless, .nonactivatingPanel],
        backing: .buffered,
        defer: false
    )
    panel.level = .popUpMenu
    panel.isFloatingPanel = true
    panel.hidesOnDeactivate = false
    panel.backgroundColor = .clear
    panel.isOpaque = false

    let bridge = KeyBridge(logFile: logFile)
    let ucc = WKUserContentController()
    ucc.add(bridge, name: "keylog")
    let config = WKWebViewConfiguration()
    config.userContentController = ucc
    let webview = WKWebView(frame: NSRect(x: 0, y: 0, width: 320, height: 180), configuration: config)
    webview.setValue(false, forKey: "drawsBackground")
    let html = """
    <html><body style="font-family:-apple-system;padding:16px;background:rgba(28,28,32,0.86);color:#F9FAFB;border-radius:14px">
    <b>S8-E2E HUD 按键吞吐冒烟</b><br>请在 TextEdit 保持输入焦点下, 物理按下 <kbd>1</kbd> / <kbd>Esc</kbd>
    <script>
    document.addEventListener('keydown', e => {
      window.webkit.messageHandlers.keylog.postMessage(e.key);
      e.preventDefault();
    });
    </script></body></html>
    """
    webview.loadHTMLString(html, baseURL: nil)
    panel.contentView = webview

    logLine(logFile, "[HUD-STATE] canBecomeKey=\(panel.canBecomeKey) canBecomeMain=\(panel.canBecomeMain) styleMask=\(panel.styleMask.rawValue)")
    panel.makeKeyAndOrderFront(nil)
    logLine(logFile, "[HUD-STATE] isKeyWindow=\(panel.isKeyWindow) appIsActive=\(app.isActive)")

    bridge.onDone = {
        logLine(logFile, "[HUD-DONE] exiting on Esc")
        exit(0)
    }
    DispatchQueue.main.asyncAfter(deadline: .now() + 60) {
        logLine(logFile, "[HUD-DONE] timeout 60s")
        exit(0)
    }
    app.run()
}

// ----------------------------------------------------------------------------
// main
// ----------------------------------------------------------------------------
let args = CommandLine.arguments
guard args.count >= 2 else {
    print("usage: step0b_e2e serve|probe|hud [args...]")
    exit(1)
}
switch args[1] {
case "serve":
    runServe(
        logFile: args.count > 2 ? args[2] : "/tmp/pf_step0b_serve.log",
        guardRead: !(args.contains("noguard"))
    )
case "probe-plain":
    runProbePlain(bundleIds: Array(args.dropFirst(2)))
case "serve-refuse":
    runServeRefuse(logFile: args.count > 2 ? args[2] : "/tmp/pf_step0b_refuse.log")
case "serve-reown":
    let delay = args.count > 3 ? (Int(args[3]) ?? 200) : 200
    runServeReown(logFile: args.count > 2 ? args[2] : "/tmp/pf_step0b_reown.log", reownDelayMs: delay)
case "probe":
    if args.count > 2 && args[2] == "--guard" {
        runProbe(bundleIds: [], probeGuardTest: true)
    } else {
        runProbe(bundleIds: Array(args.dropFirst(2)), probeGuardTest: false)
    }
case "hud":
    runHud(logFile: args.count > 2 ? args[2] : "/tmp/pf_step0b_hud.log")
default:
    print("unknown mode \(args[1])")
    exit(1)
}
