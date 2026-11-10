import XCTest
@testable import MatrixClientMac

final class NavigationTests: XCTestCase {
    @MainActor
    func testHomeExcludesSpaceRoomsWithoutDiscardingDrafts() {
        let model = ClientModel(preview: true)
        model.drafts["preview-room"] = "Unsent draft"
        model.selectSpace(nil)
        XCTAssertEqual(model.visibleRooms.map(\.id), ["preview-notes"])
        XCTAssertNil(model.selectedRoom)
        XCTAssertEqual(model.drafts["preview-room"], "Unsent draft")
        model.selectSpace("preview-space")
        XCTAssertEqual(Set(model.visibleRooms.map(\.id)), ["preview-room", "preview-research"])
    }

    @MainActor
    func testHomeWaitsForSpaceMembership() {
        let model = ClientModel(preview: true)
        model.selectSpace(nil)
        model.spacesLoaded = false
        XCTAssertTrue(model.visibleRooms.isEmpty)
    }
}
