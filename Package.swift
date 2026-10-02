// swift-tools-version: 6.0
import PackageDescription

// The Open World Format's Apple-surface package. The manifest lives at
// the repository root because SwiftPM requires it at the checkout root
// of anything resolved by tag; the sources and tests stay in swift/.
// Releases are bare semver tags (v0.1.0) — the per-language js-v* and
// rust-v* tags don't parse as versions, so the bare v-namespace is
// this package's.
let package = Package(
    name: "openworldformat",
    platforms: [
        .macOS(.v14),
        .iOS(.v17),
        .visionOS(.v1),
    ],
    products: [
        .library(name: "OpenWorldFormat", targets: ["OpenWorldFormat"]),
    ],
    targets: [
        .target(
            name: "OpenWorldFormat",
            path: "swift/Sources/OpenWorldFormat"
        ),
        .testTarget(
            name: "OpenWorldFormatTests",
            dependencies: ["OpenWorldFormat"],
            path: "swift/Tests/OpenWorldFormatTests"
        ),
    ]
)
