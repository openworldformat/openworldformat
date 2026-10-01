// swift-tools-version: 6.0
import PackageDescription

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
        .target(name: "OpenWorldFormat"),
        .testTarget(name: "OpenWorldFormatTests", dependencies: ["OpenWorldFormat"]),
    ]
)
