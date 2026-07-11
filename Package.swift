// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "Cadence",
    platforms: [.macOS(.v13)],
    dependencies: [
        .package(url: "https://github.com/sparkle-project/Sparkle", from: "2.7.0"),
    ],
    targets: [
        .executableTarget(
            name: "Cadence",
            dependencies: [
                .product(name: "Sparkle", package: "Sparkle"),
            ],
            path: "Sources/Cadence",
            linkerSettings: [
                // 素のSPM実行ファイルにInfo.plistを埋め込む
                // (LSUIElement / NSAppleEventsUsageDescription をTCCに認識させるため)
                .unsafeFlags([
                    "-Xlinker", "-sectcreate",
                    "-Xlinker", "__TEXT",
                    "-Xlinker", "__info_plist",
                    "-Xlinker", "Support/Info.plist",
                ]),
            ]
        ),
        .testTarget(
            name: "CadenceTests",
            dependencies: ["Cadence"],
            path: "Tests/CadenceTests"
        ),
    ]
)
