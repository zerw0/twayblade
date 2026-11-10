// swift-tools-version: 5.10
import PackageDescription
import Foundation

let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().path
let package = Package(
    name: "MatrixClientMac",
    platforms: [.macOS(.v14)],
    products: [.executable(name: "MatrixClientMac", targets: ["MatrixClientMac"])],
    targets: [
        .systemLibrary(name: "MatrixBridgeFFI", path: "Generated/FFI"),
        .target(name: "MatrixBridge", dependencies: ["MatrixBridgeFFI"], path: "Generated/Swift",
            linkerSettings: [.linkedLibrary("matrix_client_bridge"),
                .unsafeFlags(["-L", root + "/target/debug", "-Xlinker", "-rpath", "-Xlinker", root + "/target/debug"])]),
        .executableTarget(name: "MatrixClientMac", dependencies: ["MatrixBridge"]),
        .testTarget(name: "MatrixClientMacTests", dependencies: ["MatrixClientMac"]),
    ]
)
