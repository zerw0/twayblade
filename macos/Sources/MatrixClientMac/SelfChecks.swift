#if DEBUG
import Foundation
import MatrixBridge

// CLT-only installations do not ship XCTest. These checks exercise the actual
// Swift model and generated Rust ABI without Keychain, accounts, or network I/O.
@MainActor
func runSelfChecks() throws {
    func require(_ condition: @autoclosure () -> Bool) throws {
        if !condition() { throw NSError(domain: "MatrixClient.SelfCheck", code: 1) }
    }
    let model = ClientModel(preview: true)
    model.drafts["preview-room"] = "Keep this draft"
    model.selectSpace(nil)
    try require(model.visibleRooms.map(\.id) == ["preview-notes"])
    try require(model.selectedRoom == nil)
    try require(model.drafts["preview-room"] == "Keep this draft")
    model.selectSpace("preview-space")
    try require(Set(model.visibleRooms.map(\.id)) == Set(["preview-room", "preview-research"]))
    model.selectSpace(nil)
    model.spacesLoaded = false
    try require(model.visibleRooms.isEmpty)
    var calendar = Calendar(identifier: .gregorian)
    calendar.timeZone = TimeZone(secondsFromGMT: 0)!
    func entry(_ sender: String, _ time: Int64) -> NativeMessage {
        NativeMessage(sender: sender, displayName: "Same name", body: "Sample", timestampMs: time, delivery: "")
    }
    try require(groupedMessages([entry("a", 0), entry("a", 60_000)], calendar: calendar).count == 1)
    try require(groupedMessages([entry("a", 0), entry("b", 60_000)], calendar: calendar).count == 2)
    try require(groupedMessages([entry("a", 0), entry("a", 300_000)], calendar: calendar).count == 2)
    try require(groupedMessages([entry("a", 60_000), entry("a", 0)], calendar: calendar).count == 2)
    try require(groupedMessages([entry("a", 86_390_000), entry("a", 86_410_000)], calendar: calendar).count == 2)
    model.openProfile("preview-member", displayName: "Preview member")
    try require(model.profile?.userId == "preview-member")
    model.closeProfile()
    try require(model.profile == nil && model.profileSelection == nil)
    let client = try NativeClient()
    let view = client.snapshot()
    try require(view.userId.isEmpty && view.messages.isEmpty)
    do {
        _ = try client.discover(homeserver: "http://example.org")
        throw NSError(domain: "MatrixClient.SelfCheck", code: 2)
    } catch BridgeError.Operation { /* Invalid HTTPS input is rejected in Rust. */ }
    let revisionModel = ClientModel(preview: true)
    var newer = view
    newer.revision = 2
    newer.userId = "new-session"
    revisionModel.apply(newer)
    var older = view
    older.revision = 1
    older.userId = "stale-session"
    revisionModel.apply(older)
    try require(revisionModel.userID == "new-session")
    client.stop()
}
#endif
