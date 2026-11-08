import Foundation
import Observation
import TillCore

/// The Rust engine and what it last published, for SwiftUI to draw. The
/// engine owns all behaviour (docs/ios-plan.md §2); this class only keeps
/// its updates on the main actor and passes the cashier's actions back.
@MainActor
@Observable
final class Core {
    private(set) var version: String?
    private(set) var updates: [String] = []
    private var engine: Engine?

    init(secrets: Platform = DevicePlatform()) {
        let listener = Listener(core: self)
        engine = Engine(config: Core.config(), platform: secrets, listener: listener)
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

    /// Where the engine keeps its files: the app's own Application Support
    /// for now; the App Group's container once the extension shares it (I5).
    private static func config() -> EngineConfig {
        let directory = (try? FileManager.default.url(
            for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true
        )) ?? FileManager.default.temporaryDirectory
        return EngineConfig(dataDir: directory.path)
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
