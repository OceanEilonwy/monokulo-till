import Foundation
import Observation
import TillCore
import UIKit

/// The Rust engine and what it last published, for SwiftUI to draw. The
/// engine owns all behaviour (docs/ios-plan.md §2); this class only keeps
/// its updates on the main actor, carries out its effects, and passes the
/// cashier's actions back.
@MainActor
@Observable
final class Core {
    private(set) var version: String?
    /// The latest updates as text, for phase I0's core page.
    private(set) var updates: [String] = []
    /// Where the app is (`Update.screen`); nil until the engine says.
    private(set) var screen: Screen?
    /// Onboarding's step, while setup is on screen (`Update.onboarding`).
    private(set) var onboarding: OnboardingView?
    /// A network call is under way, and what went wrong last (`Update.work`).
    private(set) var busy = false
    private(set) var problem: String?
    /// What the build chose: the points of sale's redirects among it.
    let config: EngineConfig
    private var engine: Engine?
    private let signIn: SignInSession

    init(secrets: Platform = DevicePlatform(), push: PushPlatform = ApnsPush()) {
        config = Core.config()
        signIn = SignInSession()
        let listener = Listener(core: self)
        engine = Engine(config: config, platform: secrets, push: push, listener: listener)
    }

    /// Which screen draws the app now.
    var route: Route { Route.of(screen) }

    func send(_ action: Action) {
        engine?.send(action: action)
    }

    /// A link that opened the app: a connect link, or a sign-in coming back.
    func open(_ url: URL) {
        switch IncomingLink.of(url, square: config.square.redirectUri, clover: config.clover.redirectUri) {
        case .connect(let link): send(.connectLink(url: link))
        case .signInReturned(let link): send(.signInReturned(url: link))
        }
    }

    /// The app came to the front: the engine publishes everything again,
    /// and hears what the system allows now.
    func opened() {
        send(.opened)
        Task { await readPermissions() }
    }

    func backgrounded() {
        send(.backgrounded)
    }

    /// Tells the engine whether alerts are allowed.
    func readPermissions() async {
        let status = await AlertsPermission.status()
        send(.permissions(alerts: AlertsPermission.allows(status), bubble: false))
    }

    fileprivate func receive(_ update: Update) {
        switch update {
        case .core(let version): self.version = version
        case .screen(let screen): self.screen = screen
        case .onboarding(let view): onboarding = view
        case .work(let busy, let problem):
            self.busy = busy
            self.problem = problem
        default: break
        }
        updates.append(String(describing: update))
        if updates.count > Self.keptUpdates {
            updates.removeFirst(updates.count - Self.keptUpdates)
        }
    }

    fileprivate func perform(_ effect: Effect) {
        switch effect {
        case .openUrl(let url):
            guard let page = URL(string: url) else { return }
            signIn.start(page, fallback: config.square.redirectUri) { [weak self] returned in
                guard let returned else { return }
                self?.send(.signInReturned(url: returned.absoluteString))
            }
        case .copyText(let text):
            UIPasteboard.general.string = text
        default:
            break
        }
    }

    private static let keptUpdates = 50

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

/// Hands the engine's updates and effects, which arrive on its own thread,
/// to the main actor.
private final class Listener: EngineListener, @unchecked Sendable {
    private weak var core: Core?

    init(core: Core) {
        self.core = core
    }

    func onUpdate(update: Update) {
        Task { @MainActor [weak core] in core?.receive(update) }
    }

    func onEffect(effect: Effect) {
        Task { @MainActor [weak core] in core?.perform(effect) }
    }

    func onLog(message: String) {
        NSLog("till-core: %@", message)
    }
}
