import AppKit
import Foundation
import MatrixBridge
import SwiftUI

extension NativeRoom: Identifiable {}
extension NativeSpace: Identifiable {}

@MainActor
final class ClientModel: ObservableObject {
    @Published var userID = ""
    @Published var rooms: [NativeRoom] = []
    @Published var spaces: [NativeSpace] = []
    @Published var messages: [NativeMessage] = []
    @Published var spacesLoaded = false
    @Published var selectedSpace: String?
    @Published var selectedRoom: String?
    @Published var search = ""
    @Published var homeserver = "https://matrix.org"
    @Published var username = ""
    @Published var password = ""
    @Published var options: NativeLoginOptions?
    @Published var status = ""
    @Published var timelineStatus = ""
    @Published var timelineReady = false
    @Published var busy = false
    @Published var sending = Set<String>()
    @Published var paginating = false
    @Published var profileSelection: ProfileSelection?
    @Published var profile: NativeProfile?
    @Published var profileLoading = false
    @Published var profileError = ""
    private var profileRequest = UUID()
    private var profileTask: Task<Void, Never>?
    @Published var drafts: [String: String] = [:]
    let preview: Bool
    private var bridge: NativeClient?
    private var updates: Task<Void, Never>?
    private var lastRevision: UInt64?
    private var selectionTask: Task<Void, Never>?

    init(preview override: Bool? = nil) {
        preview = override ?? (CommandLine.arguments.contains("--preview") || CommandLine.arguments.contains("--preview-home") || CommandLine.arguments.contains("--preview-profile"))
        if preview { loadPreview() }
    }

    var currentRoom: NativeRoom? { rooms.first { $0.id == selectedRoom } }
    var currentSpace: NativeSpace? { spaces.first { $0.id == selectedSpace } }
    var navigationTitle: String { currentSpace?.name ?? "Home" }
    var visibleRooms: [NativeRoom] {
        rooms.filter { !$0.isSpace && visible($0.id) &&
            (search.isEmpty || $0.name.localizedCaseInsensitiveContains(search)) }
    }
    func visible(_ id: String) -> Bool {
        if let selectedSpace {
            return spaces.first { $0.id == selectedSpace }?.roomIds.contains(id) == true
        }
        return spacesLoaded && !spaces.contains { $0.roomIds.contains(id) }
    }
    func unread(_ space: NativeSpace) -> UInt64 {
        rooms.filter { !$0.isSpace && space.roomIds.contains($0.id) }
            .reduce(0) { total, room in
                let (sum, overflow) = total.addingReportingOverflow(room.unread)
                return overflow ? UInt64.max : sum
            }
    }

    func start() async {
        guard !preview, bridge == nil else { return }
        busy = true
        status = "Opening secure session…"
        do {
            let client = try await Task.detached { try NativeClient() }.value
            bridge = client
            let restored = try await Task.detached { try client.restore() }.value
            status = restored ? "Connecting…" : "Sign in to your homeserver."
            apply(client.snapshot())
            observe(client)
        } catch { status = message(error) }
        busy = false
    }

    private func observe(_ client: NativeClient) {
        updates?.cancel()
        updates = Task { [weak self] in
            while !Task.isCancelled {
                do {
                    let snapshot = try await Task.detached { try client.nextSnapshot() }.value
                    guard !Task.isCancelled else { break }
                    self?.apply(snapshot)
                } catch {
                    self?.status = "Updates stopped. Restart the application to reconnect."
                    break
                }
            }
        }
    }

    func apply(_ snapshot: NativeSnapshot) {
        guard lastRevision == nil || snapshot.revision > lastRevision! else { return }
        lastRevision = snapshot.revision
        if userID != snapshot.userId { closeProfile() }
        userID = snapshot.userId
        rooms = snapshot.rooms
        spaces = snapshot.spaces
        spacesLoaded = snapshot.spacesLoaded
        if !snapshot.syncStatus.isEmpty { status = snapshot.syncStatus }
        if let selectedSpace, !spaces.contains(where: { $0.id == selectedSpace }) {
            self.selectedSpace = nil
        }
        if let selectedRoom, !visible(selectedRoom) {
            self.selectedRoom = nil
            messages = []
            timelineReady = false
        }
        if snapshot.selectedRoom == selectedRoom {
            messages = snapshot.messages
            timelineReady = snapshot.timelineReady
            timelineStatus = snapshot.timelineStatus
        }
    }

    func discover() async {
        guard !busy, !preview, let bridge else { return }
        busy = true
        status = "Finding sign-in options…"
        let address = homeserver
        do {
            options = try await Task.detached { try bridge.discover(homeserver: address) }.value
            homeserver = options?.homeserver ?? address
            status = options?.password == true ? "" : "Browser sign-in is not implemented yet."
        } catch { status = message(error) }
        busy = false
    }

    func login() async {
        guard !busy, !preview, let bridge else { return }
        busy = true
        status = "Signing in…"
        let address = homeserver, name = username, secret = password
        password = ""
        do {
            try await Task.detached { try bridge.login(homeserver: address, username: name, password: secret) }.value
            apply(bridge.snapshot())
        } catch { status = message(error) }
        busy = false
    }

    func selectSpace(_ id: String?) {
        selectedSpace = id
        search = ""
        if let selectedRoom, !visible(selectedRoom) {
            self.selectedRoom = nil
            selectionTask?.cancel()
            messages = []
            timelineReady = false
        }
    }

    func selectRoom(_ id: String) {
        selectedRoom = id
        messages = []
        timelineReady = false
        timelineStatus = preview ? "Offline preview · sending disabled" : "Loading messages…"
        guard !preview, let bridge else { return }
        // Chain requests so rapid selections cannot reach Rust out of order.
        let previous = selectionTask
        selectionTask = Task {
            await previous?.value
            guard !Task.isCancelled, self.selectedRoom == id else { return }
            do { try await Task.detached { try bridge.selectRoom(roomId: id) }.value }
            catch { if self.selectedRoom == id { self.timelineStatus = message(error) } }
        }
    }

    func send() async {
        guard !preview, !busy, timelineReady, let bridge, let id = selectedRoom,
              !sending.contains(id), let body = drafts[id],
              !body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        sending.insert(id)
        defer { sending.remove(id) }
        do {
            try await Task.detached { try bridge.sendText(roomId: id, body: body) }.value
            if drafts[id] == body { drafts[id] = nil }
        } catch { if selectedRoom == id { timelineStatus = message(error) } }
    }

    func loadEarlier() async {
        guard !preview, !paginating, let bridge, let id = selectedRoom else { return }
        paginating = true
        defer { paginating = false }
        do {
            let atStart = try await Task.detached { try bridge.loadEarlier(roomId: id) }.value
            if selectedRoom == id { timelineStatus = atStart ? "Beginning of conversation" : "" }
        } catch { if selectedRoom == id { timelineStatus = message(error) } }
    }

    func logout() async {
        guard !preview, !busy, let bridge else { return }
        busy = true
        defer { busy = false }
        do {
            selectionTask?.cancel()
            try await Task.detached { try bridge.logout() }.value
            closeProfile()
            selectedRoom = nil
            selectedSpace = nil
            drafts = [:]
            messages = []
            options = nil
            apply(bridge.snapshot())
            status = "Signed out."
        } catch { status = message(error) }
    }

    func openProfile(_ id: String, displayName: String) {
        closeProfile()
        profileSelection = ProfileSelection(id: id, displayName: displayName)
        if preview {
            profile = NativeProfile(userId: id, displayName: displayName, roomName: currentRoom?.name,
                                    roomDisplayName: nil, serverProfileAvailable: true)
            return
        }
        guard let bridge else { profileError = "Sign in to view profiles."; return }
        let request = profileRequest, account = userID, room = selectedRoom
        profileLoading = true
        profileTask = Task {
            defer { if self.profileRequest == request { self.profileLoading = false } }
            do {
                let result = try await Task.detached { try bridge.profile(userId: id, roomId: room) }.value
                guard !Task.isCancelled, self.profileRequest == request, self.userID == account else { return }
                self.profile = result
            } catch {
                guard !Task.isCancelled, self.profileRequest == request, self.userID == account else { return }
                self.profileError = message(error)
            }
        }
    }

    func closeProfile() {
        profileRequest = UUID()
        profileTask?.cancel()
        profileTask = nil
        profileSelection = nil
        profile = nil
        profileError = ""
        profileLoading = false
    }

    func stop() {
        closeProfile()
        updates?.cancel()
        selectionTask?.cancel()
        bridge?.stop()
    }

    private func message(_ error: Error) -> String {
        if case let BridgeError.Operation(message) = error { return message }
        return "The operation could not be completed."
    }

    private func loadPreview() {
        userID = "Offline interface preview"
        status = "Preview · no account connected"
        spacesLoaded = true
        spaces = [NativeSpace(id: "preview-space", name: "Design team", level: 0,
                              roomIds: ["preview-room", "preview-research"])]
        rooms = [
            NativeRoom(id: "preview-room", name: "General", unread: 3, isSpace: false),
            NativeRoom(id: "preview-research", name: "Research", unread: 0, isSpace: false),
            NativeRoom(id: "preview-notes", name: "Personal notes", unread: 0, isSpace: false)
        ]
        selectedSpace = CommandLine.arguments.contains("--preview-home") ? nil : "preview-space"
        selectedRoom = selectedSpace == nil ? "preview-notes" : "preview-room"
        timelineStatus = "Offline preview · sending disabled"
        messages = [
            NativeMessage(sender: "preview-member", displayName: "Preview member",
                body: "This is a native SwiftUI interface. The shared Matrix core remains in Rust.",
                timestampMs: 1_794_300_000_000, delivery: ""),
            NativeMessage(sender: "preview-member", displayName: "Preview member",
                body: "Messages sent together share one name and timestamp.",
                timestampMs: 1_794_300_030_000, delivery: ""),
            NativeMessage(sender: userID, displayName: "You",
                body: "Shared layout, native controls on each platform.",
                timestampMs: 1_794_300_060_000, delivery: ""),
            NativeMessage(sender: userID, displayName: "You", body: "And keep their own message bubbles.",
                timestampMs: 1_794_300_080_000, delivery: "")
        ]
    }
}
