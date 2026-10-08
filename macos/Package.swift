// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "AIEmailSearch",
    platforms: [.macOS(.v13)],
    products: [.executable(name: "AIEmailSearch", targets: ["MailSearchApp"])],
    targets: [
        .target(name: "MailSearchCore"),
        .executableTarget(name: "MailSearchApp", dependencies: ["MailSearchCore"]),
        .executableTarget(name: "MailSearchChecks", dependencies: ["MailSearchCore"], path: "Tests/MailSearchCoreTests")
    ]
)
