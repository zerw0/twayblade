import AppKit
import SwiftUI

@main
@MainActor
enum MatrixClientApp {
    static func main() {
        #if DEBUG
        if CommandLine.arguments.contains("--self-test") {
            do { try runSelfChecks(); print("SwiftUI/bridge self-checks passed"); exit(0) }
            catch { fputs("SwiftUI/bridge self-check failed\n", stderr); exit(1) }
        }
        #endif
        let app = NSApplication.shared
        let delegate = AppDelegate()
        app.delegate = delegate
        app.setActivationPolicy(.regular)
        app.run()
        withExtendedLifetime(delegate) {}
    }
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private let model = ClientModel()
    private var window: NSWindow?

    func applicationDidFinishLaunching(_ notification: Notification) {
        installMenus()
        let content = RootView().environmentObject(model)
        let controller = NSHostingController(rootView: content)
        let window = NSWindow(contentViewController: controller)
        window.styleMask = [.titled, .closable, .miniaturizable, .resizable]
        window.title = "Matrix Client"
        window.setContentSize(NSSize(width: 1120, height: 760))
        window.minSize = NSSize(width: 760, height: 540)
        window.titlebarAppearsTransparent = true
        window.isReleasedWhenClosed = false
        window.center()
        self.window = window
        window.makeKeyAndOrderFront(nil)
        NSApplication.shared.activate(ignoringOtherApps: true)
        Task { await model.start() }
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        window?.makeKeyAndOrderFront(nil)
        return true
    }

    func applicationWillTerminate(_ notification: Notification) { model.stop() }

    private func installMenus() {
        let menu = NSMenu()
        let appItem = NSMenuItem()
        let appMenu = NSMenu(title: "Matrix Client")
        appMenu.addItem(withTitle: "About Matrix Client", action: #selector(NSApplication.orderFrontStandardAboutPanel(_:)), keyEquivalent: "")
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: "Hide Matrix Client", action: #selector(NSApplication.hide(_:)), keyEquivalent: "h")
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: "Quit Matrix Client", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        appItem.submenu = appMenu
        menu.addItem(appItem)
        let edit = NSMenuItem()
        let editMenu = NSMenu(title: "Edit")
        for (title, action, key) in [
            ("Undo", "undo:", "z"), ("Cut", "cut:", "x"), ("Copy", "copy:", "c"),
            ("Paste", "paste:", "v"), ("Select All", "selectAll:", "a")
        ] { editMenu.addItem(withTitle: title, action: Selector(action), keyEquivalent: key) }
        editMenu.addItem(.separator())
        let find = editMenu.addItem(withTitle: "Find Conversation", action: #selector(findConversation), keyEquivalent: "f")
        find.target = self
        edit.submenu = editMenu
        menu.addItem(edit)
        let windows = NSMenuItem()
        let windowMenu = NSMenu(title: "Window")
        windowMenu.addItem(withTitle: "Minimize", action: #selector(NSWindow.performMiniaturize(_:)), keyEquivalent: "m")
        windowMenu.addItem(withTitle: "Zoom", action: #selector(NSWindow.performZoom(_:)), keyEquivalent: "")
        windows.submenu = windowMenu
        menu.addItem(windows)
        NSApplication.shared.windowsMenu = windowMenu
        NSApplication.shared.mainMenu = menu
    }

    @objc private func findConversation() {
        NotificationCenter.default.post(name: .focusConversationSearch, object: nil)
    }
}

extension Notification.Name {
    static let focusConversationSearch = Notification.Name("MatrixClient.FocusSearch")
}

struct RootView: View {
    @EnvironmentObject private var model: ClientModel
    var body: some View {
        Group {
            if model.userID.isEmpty { SignInView() } else { WorkspaceView() }
        }
        .onAppear {
            NSApplication.shared.activate(ignoringOtherApps: true)
            if model.preview && CommandLine.arguments.contains("--preview-profile") {
                model.openProfile("preview-member", displayName: "Preview member")
            }
            capturePreviewIfRequested()
        }
    }

    private func capturePreviewIfRequested() {
        guard model.preview,
              let index = CommandLine.arguments.firstIndex(of: "--capture"),
              CommandLine.arguments.indices.contains(index + 1) else { return }
        let path = CommandLine.arguments[index + 1]
        DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
            guard let window = NSApplication.shared.windows.first(where: { $0.isVisible }) else {
                NSApplication.shared.terminate(nil); return
            }
            // Capture only this synthetic preview window, including system chrome.
            let capture = Process()
            capture.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
            capture.arguments = ["-x", "-l", String(window.windowNumber), path]
            do {
                try capture.run()
                capture.waitUntilExit()
            } catch { /* Preview capture is optional; never fall back to the desktop. */ }
            model.closeProfile()
            DispatchQueue.main.async { NSApplication.shared.terminate(nil) }
        }
    }
}

struct SignInView: View {
    @EnvironmentObject private var model: ClientModel
    var body: some View {
        VStack(spacing: 20) {
            Image(systemName: "bubble.left.and.bubble.right.fill")
                .font(.system(size: 42)).foregroundStyle(.tint)
            Text("Welcome to Matrix").font(.title.bold())
            Text("Your conversations. Your server.").foregroundStyle(.secondary)
            VStack(alignment: .leading, spacing: 12) {
                if model.options == nil {
                    Text("Homeserver").font(.caption.weight(.semibold))
                    TextField("https://matrix.example.org", text: $model.homeserver)
                        .textFieldStyle(.roundedBorder).onSubmit { Task { await model.discover() } }
                    Button("Continue") { Task { await model.discover() } }
                        .buttonStyle(.borderedProminent).disabled(model.busy)
                } else {
                    HStack {
                        Text(model.homeserver).font(.caption).lineLimit(1)
                        Spacer()
                        Button("Change") { model.options = nil; model.password = "" }.disabled(model.busy)
                    }
                    if model.options?.password == true {
                        TextField("Username or Matrix ID", text: $model.username).textFieldStyle(.roundedBorder)
                        SecureField("Password", text: $model.password).textFieldStyle(.roundedBorder)
                            .onSubmit { Task { await model.login() } }
                        Button("Sign In") { Task { await model.login() } }
                            .buttonStyle(.borderedProminent)
                            .disabled(model.busy || model.username.trimmingCharacters(in: .whitespaces).isEmpty || model.password.isEmpty)
                    }
                }
            }.disabled(model.busy)
            if model.busy { ProgressView().controlSize(.small) }
            Text(model.status).font(.caption).foregroundStyle(.secondary).multilineTextAlignment(.center)
        }.frame(maxWidth: 340).padding(40).frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
