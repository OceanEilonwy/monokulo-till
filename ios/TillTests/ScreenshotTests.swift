import SwiftUI
import Testing
import UIKit
@testable import Till

/// Screenshots of the key screens and states on whichever simulator the
/// tests run on, so a pull request's UI can be looked over without an
/// iPhone or an iPad to hand: ios.yml collects them as an artifact, and
/// screenshots.yml sets them beside Android's (tools/screenshot-compare).
///
/// Every screen is the app's own, drawn from the engine's previews
/// (`previewShift`, `previewSale`, `previewOnboarding`, `previewSettings`…)
/// through a ``Core`` with no engine, so the words and states are the ones
/// a till would show. Android's ScreenshotTest draws the same sample till
/// under the same names.
///
/// Each screen lands in `$SNAPSHOT_DIR/iphone/` or `…/ipad/` as
/// `<name>-light.png` and `<name>-dark.png`, the whole window; `shots.txt`
/// lists the names in order and `device.txt` names the simulator. The test
/// runs only when that folder is named (CI sets it through
/// `TEST_RUNNER_SNAPSHOT_DIR`).
@MainActor
@Suite(.serialized, .enabled(if: Screenshots.root != nil, "SNAPSHOT_DIR names no folder"))
struct ScreenshotTests {
    @Test func everyKeyScreen() async throws {
        let root = try #require(Screenshots.root)
        let folder = root.appendingPathComponent(Screenshots.ipad ? "ipad" : "iphone", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let scene = try #require(
            UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first,
            "the test host has a window scene"
        )
        let shots = Screenshots.shots(ipad: Screenshots.ipad)
        for shot in shots {
            for dark in [false, true] {
                try await Screenshots.take(shot, dark: dark, scene: scene, into: folder)
            }
        }
        try (shots.map(\.name).joined(separator: "\n") + "\n")
            .write(to: folder.appendingPathComponent("shots.txt"), atomically: true, encoding: .utf8)
        try (Screenshots.deviceName + "\n")
            .write(to: folder.appendingPathComponent("device.txt"), atomically: true, encoding: .utf8)
    }
}

/// One screenshot: the app's window showing what the engine published.
struct Shot {
    let name: String
    var landscape = false
    /// What the engine would have published, in order.
    var updates: [Update] = []
    /// A screen drawn on its own, where no `Screen` reaches it.
    var screen: (@MainActor () -> AnyView)?
}

@MainActor
enum Screenshots {
    nonisolated static var root: URL? {
        guard let path = ProcessInfo.processInfo.environment["SNAPSHOT_DIR"], !path.isEmpty else { return nil }
        return URL(fileURLWithPath: path, isDirectory: true)
    }

    static var ipad: Bool { UIDevice.current.userInterfaceIdiom == .pad }

    /// "iPhone 17 Pro · iOS 27.0": the simulator's name and its system.
    static var deviceName: String {
        let name = ProcessInfo.processInfo.environment["SIMULATOR_DEVICE_NAME"] ?? (ipad ? "iPad" : "iPhone")
        return "\(name) · iOS \(UIDevice.current.systemVersion)"
    }

    // MARK: The sample till (Android's ScreenshotTest has the same)

    /// The clock the sample sales are read at.
    static let now: Int64 = 1_761_000_600
    static let platform = MemoryPlatform()

    /// A shift's worth of sales, newest first, one in each state that
    /// looks different.
    static let sales: [PreviewSale] = [
        PreviewSale(
            orderId: "w1", phase: .waiting, amount: "8.40", reference: "m2Qe", expiresAt: now + 840,
            receipt: true, customerName: "Sam", createdAt: now - 60
        ),
        PreviewSale(
            orderId: "s1", phase: .seen, amount: "12.00", reference: "k7Pa", expiresAt: now + 600,
            customerName: "Ana", createdAt: now - 120, seenAt: now - 30
        ),
        PreviewSale(
            orderId: "c1", phase: .confirming, amount: "23.75", reference: "r4Tn", expiresAt: now + 300,
            customerName: "Lee", createdAt: now - 300, seenAt: now - 240,
            confirmations: 4, confirmationsRequired: 10
        ),
        PreviewSale(
            orderId: "p1", phase: .paid, amount: "4.50", reference: "b9Lw", expiresAt: now + 300,
            receipt: true, customerName: "Mia", createdAt: now - 900, seenAt: now - 840, paidAt: now - 600
        ),
        PreviewSale(
            orderId: "e1", phase: .expired, amount: "6.20", reference: "x3Hd", expiresAt: now - 1_200,
            customerName: "Joe", createdAt: now - 2_100, endedAt: now - 1_200
        ),
        PreviewSale(
            orderId: "o1", phase: .paid, amount: "15.00", reference: "z8Qc", expiresAt: now - 1_500,
            found: false, deviceName: "Bar iPad", createdAt: now - 2_400, seenAt: now - 2_350, paidAt: now - 2_200
        ),
    ]

    static func preview(
        _ sales: [PreviewSale], ipad: Bool, live: Bool = true, problem: String? = nil
    ) -> TillPreview {
        TillPreview(
            salesJson: previewSalesJson(sales: sales),
            now: now,
            device: ipad ? .ipad : .iphone,
            till: .square,
            store: "The Bean Room",
            location: "Front Counter",
            live: live,
            problem: problem,
            startedAt: now - 3_600
        )
    }

    // MARK: The screens

    static func shots(ipad: Bool) -> [Shot] {
        let device: Device = ipad ? .ipad : .iphone
        let full = preview(sales, ipad: ipad)
        let shift = previewShift(preview: full, platform: platform).map { Update.shift(view: $0) }
        let empty = previewShift(preview: preview([], ipad: ipad), platform: platform).map { Update.shift(view: $0) }
        let offline = previewShift(
            preview: preview(sales, ipad: ipad, live: false, problem: "Couldn't reach the store."), platform: platform
        ).map { Update.shift(view: $0) }
        let settings = Update.settings(view: previewSettings(device: device, now: now))
        let wakeUps = previewWakeUps(device: device, now: now)

        func sale(_ orderId: String) -> Update? {
            previewSale(preview: full, orderId: orderId, platform: platform)
        }
        func till(_ screen: Screen, _ updates: Update?...) -> [Update] {
            [Update.screen(screen: screen)] + updates.compactMap { $0 }
        }
        func step(_ step: PreviewStep, _ screen: Screen) -> [Update] {
            [
                .onboarding(view: previewOnboarding(step: step, device: device, platform: platform)),
                .screen(screen: screen),
            ]
        }
        let close = previewClose(preview: full, orderId: "w1", platform: platform)
            .map { Update.closeSheet(orderId: "w1", view: $0) }
        let settingsUp = till(.settings, shift, settings, .wakeUps(view: wakeUps))
        /// Settings for a till on the shop's own wallet (core.md decision
        /// 234): its Payments sections and Disconnect, as Settings draws them,
        /// without the sections above that a window shows first.
        func walletSettings(_ look: PreviewSettings) -> @MainActor () -> AnyView {
            let view = previewSettingsOf(look: look, device: device, now: now)
            return {
                AnyView(NavigationStack {
                    List {
                        if let wallet = view.wallet {
                            WalletSettings(wallet: wallet, send: { _ in })
                        }
                        Section {
                            Button(view.disconnect, role: .destructive) {}
                                .foregroundStyle(Palette.danger)
                        } footer: {
                            Text(view.disconnectFootnote)
                        }
                    }
                    .listStyle(.insetGrouped)
                    .scrollContentBackground(.hidden)
                    .readableWidth(640)
                    .background(Palette.ground)
                    .navigationTitle(view.title)
                    .navigationBarTitleDisplayMode(.inline)
                    .tint(Palette.link)
                })
            }
        }

        var shots: [Shot] = [
            Shot(name: "setup-1-welcome", updates: step(.welcome, .welcome)),
            Shot(name: "setup-2-scan", updates: step(.scan, .scan)),
            Shot(name: "setup-3-confirm", updates: step(.confirm, .confirm)),
            Shot(name: "setup-3-connected", updates: step(.connected, .connected)),
            Shot(name: "setup-4-point-of-sale", updates: step(.pointOfSale, .pointOfSale)),
            Shot(name: "setup-4-signed-in", updates: step(.signedIn, .pointOfSale)),
            Shot(name: "setup-4-test-sale", updates: step(.testSale, .pointOfSale)),
            Shot(name: "setup-4-locations", updates: step(.locations, .locations)),
            Shot(name: "setup-4-counter", updates: step(.counter, .counter)),
            Shot(name: "setup-5-alerts", updates: step(.alerts, .alerts)),
        ]
        if ipad {
            shots.append(Shot(name: "setup-5-show-sales", updates: step(.showSales, .showSales)))
        }
        // View-key mode's setup (the Vk* boards), each under its board's
        // name: B4 on iPhone, C4 (`vkPadConnected`, as Android's tablet) on iPad.
        shots += [
            Shot(name: "vk-mode", updates: step(.vkMode, .paymentsMode)),
            Shot(name: "vk-wallet", updates: step(.vkWallet, .walletEntry)),
            Shot(name: "vk-pad-request", updates: step(.vkPadRequest, .fobRequest)),
            ipad
                ? Shot(name: "vk-pad-connected", updates: step(.vkPadConnected, .fobConnected))
                : Shot(name: "vk-connected", updates: step(.vkConnected, .fobConnected)),
            Shot(name: "vk-scan-code", updates: step(.vkScanCode, .walletCode)),
            Shot(name: "vk-keys-error", updates: step(.vkKeysError, .walletKeys)),
            Shot(name: "vk-seed", updates: step(.vkSeed, .walletSeed)),
            Shot(name: "vk-this-till", updates: step(.vkThisTill, .thisTill)),
            Shot(name: "vk-settings", screen: walletSettings(.vkSettings)),
            Shot(name: "vk-settings-warning", screen: walletSettings(.vkSettingsWarning)),
            Shot(name: "vk-light-mode", updates: step(.vkLightMode, .paymentsMode)),
            Shot(name: "vk-light-server", updates: step(.vkLightServer, .lightServer)),
            Shot(name: "vk-light-this-till", updates: step(.vkLightThisTill, .thisTill)),
            Shot(name: "vk-light-settings", screen: walletSettings(.vkLightSettings)),
        ]
        shots += [
            Shot(name: "setup-6-ready", updates: step(.ready, .ready)),
            Shot(name: "till-empty", updates: till(.till, empty)),
            Shot(name: "till-shift", updates: till(.till, shift)),
            Shot(name: "till-shift-offline", updates: till(.till, offline)),
            Shot(name: "sale-waiting", updates: till(.sale(orderId: "w1"), shift, sale("w1"))),
            Shot(name: "sale-seen", updates: till(.sale(orderId: "s1"), shift, sale("s1"))),
            Shot(name: "sale-confirming", updates: till(.sale(orderId: "c1"), shift, sale("c1"))),
            Shot(name: "sale-paid", updates: till(.sale(orderId: "p1"), shift, sale("p1"))),
            Shot(name: "sale-expired", updates: till(.sale(orderId: "e1"), shift, sale("e1"))),
            Shot(name: "sale-other-till", updates: till(.sale(orderId: "o1"), shift, sale("o1"))),
            Shot(name: "sale-code", updates: till(.code(orderId: "w1"), shift, sale("w1"))),
            Shot(name: "sale-order", updates: till(.order(orderId: "p1"), shift, sale("p1"))),
            Shot(name: "sale-close-sheet", updates: till(.sale(orderId: "w1"), shift, sale("w1"), close)),
            Shot(name: "settings", updates: settingsUp),
            Shot(name: "settings-wake-ups", screen: {
                AnyView(NavigationStack {
                    WakeUpsScreen(view: wakeUps, title: "Wake-ups", send: { _ in })
                })
            }),
        ]
        if ipad {
            shots += [
                Shot(name: "landscape-setup-1-welcome", landscape: true, updates: step(.welcome, .welcome)),
                Shot(name: "landscape-setup-4-point-of-sale", landscape: true, updates: step(.pointOfSale, .pointOfSale)),
                Shot(name: "landscape-till-empty", landscape: true, updates: till(.till, empty)),
                Shot(name: "landscape-till-shift", landscape: true, updates: till(.till, shift)),
                Shot(name: "landscape-sale-waiting", landscape: true, updates: till(.sale(orderId: "w1"), shift, sale("w1"))),
                Shot(name: "landscape-sale-paid", landscape: true, updates: till(.sale(orderId: "p1"), shift, sale("p1"))),
                Shot(name: "landscape-sale-code", landscape: true, updates: till(.code(orderId: "w1"), shift, sale("w1"))),
                Shot(name: "landscape-settings", landscape: true, updates: settingsUp),
            ]
        }
        return shots
    }

    // MARK: Taking them

    /// Draws `shot` in a window over the test host's own, lets its sheets
    /// and covers go up and settle, and writes the window as a PNG.
    static func take(_ shot: Shot, dark: Bool, scene: UIWindowScene, into folder: URL) async throws {
        let core = Core(showing: shot.updates)
        let content = shot.screen?() ?? AnyView(RootView(core: core))
        let window = UIWindow(windowScene: scene)
        // The scene's own size, as the app draws it; turned on its side for
        // a landscape shot, as LayoutSnapshotTests sizes its windows.
        if shot.landscape {
            let screen = scene.coordinateSpace.bounds.size
            window.frame = CGRect(
                origin: .zero,
                size: CGSize(width: max(screen.width, screen.height), height: min(screen.width, screen.height))
            )
        }
        window.windowLevel = .normal + 1
        window.overrideUserInterfaceStyle = dark ? .dark : .light
        window.rootViewController = UIHostingController(rootView: content)
        window.makeKeyAndVisible()
        window.layoutIfNeeded()
        // Sheets and covers go up after the first frame, with the system's
        // animation; the codes draw off the main thread.
        try await Task.sleep(for: .seconds(2))
        let format = UIGraphicsImageRendererFormat(for: window.traitCollection)
        format.opaque = true
        let image = UIGraphicsImageRenderer(bounds: window.bounds, format: format).image { _ in
            _ = window.drawHierarchy(in: window.bounds, afterScreenUpdates: true)
        }
        let file = folder.appendingPathComponent("\(shot.name)-\(dark ? "dark" : "light").png")
        try #require(image.pngData()).write(to: file)
        window.rootViewController?.dismiss(animated: false)
        window.isHidden = true
        window.rootViewController = nil
        withExtendedLifetime(core) {}
    }
}
