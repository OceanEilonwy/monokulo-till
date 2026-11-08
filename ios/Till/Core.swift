import Foundation
import Observation
import TillCore
import UIKit

/// The Rust engine and what it last published, for SwiftUI to draw. The
/// engine owns all behaviour (docs/ios-plan.md §2); this class only keeps
/// its updates on the main actor and passes the cashier's actions back.
@MainActor
@Observable
final class Core {
    private(set) var version: String?
    private(set) var updates: [String] = []
    private var engine: Engine?

    init(secrets: Platform = DevicePlatform(), push: PushPlatform = ApnsPush()) {
        let listener = Listener(core: self)
        engine = Engine(config: Core.config(), platform: secrets, push: push, listener: listener)
    }

    func send(_ action: Action) {
        engine?.send(action: action)
    }

    fileprivate func receive(_ update: Update) {
        if case .core(let version) = update {
            self.version = version
        }
        updates.append(String(describing: update))
    }

    /// What the engine needs to know about this device and build
    /// (`EngineConfig`, docs/ios-plan.md §2). Where it keeps its files: the
    /// app's own Application Support for now; the App Group's container once
    /// the extension shares it (I5).
    private static func config() -> EngineConfig {
        let directory = (try? FileManager.default.url(
            for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true
        )) ?? FileManager.default.temporaryDirectory
        let device = UIDevice.current
        #if DEBUG
        let debug = true
        #else
        let debug = false
        #endif
        let format = DateFormatter.dateFormat(fromTemplate: "j", options: 0, locale: .current) ?? ""
        return EngineConfig(
            dataDir: directory.path,
            device: device.userInterfaceIdiom == .pad ? .ipad : .iphone,
            deviceName: device.name,
            appVersion: Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0",
            platform: "ios",
            debug: debug,
            utcOffset: Int32(TimeZone.current.secondsFromGMT()),
            hours24: !format.contains("a"),
            square: SquareSetup(
                applicationId: Build.squareApplicationId,
                sandbox: false,
                redirectUri: "https://till.monokulo.com/square",
                baseUrl: nil
            ),
            clover: CloverSetup(
                applicationId: Build.cloverApplicationId,
                environment: "north_america",
                redirectUri: "https://till.monokulo.com/clover",
                baseUrl: nil
            ),
            bellUrl: "https://bell.monokulo.com"
        )
    }
}

/// Hands the engine's updates, which arrive on its own thread, to the main
/// actor.
private final class Listener: EngineListener, @unchecked Sendable {
    private weak var core: Core?

    init(core: Core) {
        self.core = core
    }

    func onUpdate(update: Update) {
        Task { @MainActor [weak core] in core?.receive(update) }
    }

    func onEffect(effect: Effect) {}

    func onLog(message: String) {
        NSLog("till-core: %@", message)
    }
}
