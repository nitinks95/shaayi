// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "ShaayiTouchSpike",
    platforms: [.macOS(.v13)],
    targets: [
        .executableTarget(name: "ShaayiTouchSpike", path: "Sources/ShaayiTouchSpike")
    ]
)
