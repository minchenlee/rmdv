// One process-wide WKWebView, driven by the existing bounded JSON-lines protocol.
import AppKit
import WebKit

let resources = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
func reply(_ object: [String: Any]) {
    guard let data = try? JSONSerialization.data(withJSONObject: object) else { return }
    FileHandle.standardOutput.write(data)
    FileHandle.standardOutput.write(Data([10]))
}

final class Assets: NSObject, WKURLSchemeHandler {
    func webView(_ webView: WKWebView, start task: WKURLSchemeTask) {
        guard let url = task.request.url, url.host == "runtime" else {
            task.didFailWithError(NSError(domain: "rmdv", code: 1)); return
        }
        let allowed = ["/mermaid.min.js": "text/javascript", "/native.js": "text/javascript"]
        guard let mime = allowed[url.path],
              let data = try? Data(contentsOf: resources.appendingPathComponent(String(url.path.dropFirst()))) else {
            task.didFailWithError(NSError(domain: "rmdv", code: 2)); return
        }
        task.didReceive(URLResponse(url: url, mimeType: mime, expectedContentLength: data.count, textEncodingName: "utf-8"))
        task.didReceive(data)
        task.didFinish()
    }
    func webView(_ webView: WKWebView, stop task: WKURLSchemeTask) {}
}

final class Renderer: NSObject, WKScriptMessageHandler, WKNavigationDelegate {
    let assets = Assets()
    var web: WKWebView!
    var window: NSWindow!
    var ready = false
    var pending: [[String: Any]] = []
    var active = false
    var wantsRaster = false

    override init() {
        super.init()
        let config = WKWebViewConfiguration()
        config.websiteDataStore = .nonPersistent()
        config.preferences.javaScriptCanOpenWindowsAutomatically = false
        config.setURLSchemeHandler(assets, forURLScheme: "rmdv-mermaid")
        config.userContentController.add(self, name: "ready")
        config.userContentController.add(self, name: "render")
        web = WKWebView(frame: NSRect(x: 0, y: 0, width: 1600, height: 1200), configuration: config)
        web.navigationDelegate = self
        if #available(macOS 12.0, *) {
            web.underPageBackgroundColor = .clear
        }
        // Keep the render surface attached to a window for WebKit layout and
        // snapshots, but outside the visible desktop and without focus.
        window = NSWindow(contentRect: NSRect(x: -32000, y: -32000, width: 1600, height: 1200), styleMask: .borderless, backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.isOpaque = false
        window.backgroundColor = .clear
        window.collectionBehavior = [.stationary, .ignoresCycle]
        window.ignoresMouseEvents = true
        window.contentView = web
        window.orderBack(nil)
        do {
            let aws = try String(contentsOf: resources.appendingPathComponent("aws-icons.json"), encoding: .utf8).replacingOccurrences(of: "</", with: "<\\/")
            let logos = try String(contentsOf: resources.appendingPathComponent("logos-icons.json"), encoding: .utf8).replacingOccurrences(of: "</", with: "<\\/")
            let html = """
            <!doctype html><meta charset="utf-8">
            <meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src rmdv-mermaid:; style-src 'unsafe-inline'; img-src data:; font-src data:; connect-src 'none'">
            <style>html,body{margin:0;background:transparent}#diagram{display:inline-block}svg{display:block}</style>
            <script type="application/json" id="aws-icons">\(aws)</script>
            <script type="application/json" id="logos-icons">\(logos)</script>
            <div id="diagram"></div>
            <script src="rmdv-mermaid://runtime/mermaid.min.js"></script>
            <script src="rmdv-mermaid://runtime/native.js"></script>
            """
            web.loadHTMLString(html, baseURL: URL(string: "rmdv-mermaid://runtime/"))
        } catch { reply(["error": "WebKit assets: \(error.localizedDescription)"]); exit(1) }
    }

    func enqueue(_ request: [String: Any]) {
        pending.append(request)
        renderNext()
    }
    func renderNext() {
        guard ready, !active, !pending.isEmpty else { return }
        active = true
        window.setContentSize(NSSize(width: 1600, height: 1200))
        web.setFrameSize(NSSize(width: 1600, height: 1200))
        let request = pending.removeFirst()
        wantsRaster = request["raster"] as? Bool ?? false
        guard let data = try? JSONSerialization.data(withJSONObject: request), let json = String(data: data, encoding: .utf8) else {
            finish(["error": "Invalid WebKit request"]); return
        }
        // The async result travels through the script-message handler. End with
        // null so evaluateJavaScript never tries to bridge the Promise itself.
        web.evaluateJavaScript("window.renderDiagram(\(json)).then(result => window.webkit.messageHandlers.render.postMessage(result)).catch(error => window.webkit.messageHandlers.render.postMessage({error: String(error.message || error)})); null;") { _, error in
            if let error = error { self.finish(["error": error.localizedDescription]) }
        }
    }
    func finish(_ result: [String: Any]) {
        guard active else { return }
        reply(result)
        active = false
        renderNext()
    }
    func userContentController(_ controller: WKUserContentController, didReceive message: WKScriptMessage) {
        guard message.frameInfo.isMainFrame else { return }
        guard let body = message.body as? [String: Any] else { return }
        if message.name == "ready" {
            if let error = body["error"] { reply(["error": "WebKit startup: \(error)"]); exit(1) }
            ready = true
            renderNext()
            return
        }
        if body["error"] != nil || !wantsRaster { finish(body); return }
        guard let width = body["width"] as? Double, let height = body["height"] as? Double,
              width >= 1, height >= 1, width <= 1600, height <= 8192, width * height <= 8_000_000 else {
            finish(["error": "WebKit raster exceeds size limit"]); return
        }
        // Match the web view and snapshot to the bounded CSS-pixel output.
        window.setContentSize(NSSize(width: width, height: height))
        web.setFrameSize(NSSize(width: width, height: height))
        guard #available(macOS 14.0, *) else {
            finish(["error": "Transparent WebKit Mermaid rendering requires macOS 14 or later"]); return
        }
        // takeSnapshot fills transparent CSS with the frame's native base color.
        // The public PDF API can explicitly preserve that base's transparency.
        let pdfConfig = WKPDFConfiguration()
        pdfConfig.rect = CGRect(x: 0, y: 0, width: width, height: height)
        pdfConfig.allowTransparentBackground = true
        web.createPDF(configuration: pdfConfig) { result in
            switch result {
            case .failure(let error):
                self.finish(["error": "WebKit PDF: \(error.localizedDescription)"])
            case .success(let data):
                guard let provider = CGDataProvider(data: data as CFData),
                      let document = CGPDFDocument(provider),
                      let page = document.page(at: 1),
                      let colorSpace = CGColorSpace(name: CGColorSpace.sRGB),
                      let context = CGContext(data: nil, width: Int(width), height: Int(height),
                        bitsPerComponent: 8, bytesPerRow: Int(width) * 4, space: colorSpace,
                        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue | CGBitmapInfo.byteOrder32Big.rawValue) else {
                    self.finish(["error": "WebKit PDF raster allocation failed"]); return
                }
                let rect = CGRect(x: 0, y: 0, width: width, height: height)
                context.clear(rect)
                context.concatenate(page.getDrawingTransform(.mediaBox, rect: rect, rotate: 0, preserveAspectRatio: true))
                context.drawPDFPage(page)
                guard let image = context.makeImage(),
                      let png = NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:]) else {
                    self.finish(["error": "WebKit PNG encoding failed"]); return
                }
                var response = body
                response["png"] = png.base64EncodedString()
                self.finish(response)
            }
        }
    }
    func webView(_ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction, decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
        let scheme = navigationAction.request.url?.scheme
        decisionHandler((scheme == "rmdv-mermaid" || scheme == "about") ? .allow : .cancel)
    }
    func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
        reply(["error": "WebKit content process terminated"])
        exit(1)
    }
}

let app = NSApplication.shared
app.setActivationPolicy(.prohibited)
let renderer = Renderer()
DispatchQueue.global(qos: .userInitiated).async {
    while let line = readLine() {
        guard let data = line.data(using: .utf8), data.count <= 80 * 1024,
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            DispatchQueue.main.async { reply(["error": "Invalid WebKit request"] ) }
            continue
        }
        DispatchQueue.main.async { renderer.enqueue(object) }
    }
    DispatchQueue.main.async {
        renderer.web.stopLoading()
        renderer.window.close()
        try? FileManager.default.removeItem(at: resources)
        app.terminate(nil)
    }
}
app.run()
