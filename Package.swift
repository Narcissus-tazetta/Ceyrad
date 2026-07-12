// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "Ceyrad",
    platforms: [.macOS(.v13)],
    dependencies: [
        .package(url: "https://github.com/sparkle-project/Sparkle", from: "2.7.0"),
    ],
    targets: [
        .executableTarget(
            name: "Ceyrad",
            dependencies: [
                .product(name: "Sparkle", package: "Sparkle"),
            ],
            path: "Sources/Ceyrad",
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
            name: "CeyradTests",
            dependencies: ["Ceyrad"],
            path: "Tests/CeyradTests"
        ),
    ]
)
