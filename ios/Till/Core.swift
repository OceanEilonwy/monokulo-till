import Foundation
import Observation
import TillCore
import UIKit
import UserNotifications

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
    /// The till's screens (`Update.shift`, `.sale`, `.closeSheet`,
    /// `.refundSheet`, `.settings`, `.wakeUps`): nil until the engine says.
    private(set) var shift: ShiftView?
    private(set) var sale: SaleScreens?
    private(set) var closeSheet: CloseSheet?
    private(set) var refundSheet: RefundSheetView?
    private(set) var settings: SettingsView?
    private(set) var wakeUps: WakeUpsView?
    /// How the next screen came: by itself (the sales moved the till), or
    /// the cashier (`Update.nav`).
    private(set) var byItself = false
    /// What the build chose: the points of sale's redirects among it.
    let config: EngineConfig
    private var engine: Engine?
    private let signIn: SignInSession
    /// Taps on a new sale's alert, which open the sale.
    private var taps: NotificationTaps?

    init(secrets: Platform = DevicePlatform(), push: PushPlatform = ApnsPush()) {
        config = Core.config()
        signIn = SignInSession()
        let listener = Listener(core: self)
        engine = Engine(config: config, platform: secrets, push: push, listener: listener)
        let taps = NotificationTaps { [weak self] orderId in
            self?.send(.openSale(orderId: orderId))
        }
        self.taps = taps
        UNUserNotificationCenter.current().delegate = taps
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

    /// Once the move to `screen` has settled (its animation, then a little),
    /// the engine hears the till rests there. Cancelled by the next move.
    func land(_ screen: Screen?) async {
        guard let screen, Route.of(screen).lands else { return }
        try? await Task.sleep(for: .milliseconds(300))
        guard !Task.isCancelled, screen == self.screen else { return }
        send(.landed(screen: screen))
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
        case .nav(let byItself, _): self.byItself = byItself
        case .shift(let view): shift = view
        case .sale(let orderId, let view, let page, let code, let order):
            sale = SaleScreens(orderId: orderId, view: view, page: page, code: code, order: order)
        case .closeSheet(let orderId, let view):
            if let orderId, let view {
                closeSheet = CloseSheet(orderId: orderId, view: view)
            } else {
                closeSheet = nil
            }
        case .refundSheet(let view): refundSheet = view
        case .settings(let view): settings = view
        case .wakeUps(let view): wakeUps = view
        case .notification(let view):
            // Set in one update only, as the sale arrives with Till in the back.
            if let alert = view.alert {
                Task { await SaleAlerts.post(alert) }
            }
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
            // A page naming no redirect ends at the chosen point of sale's.
            let chosen = chosenPos(step: posStep, settings: settings?.pointOfSale)
            let fallback = redirectUri(chosen, square: config.square.redirectUri, clover: config.clover.redirectUri)
            signIn.start(page, fallback: fallback) { [weak self] returned in
                guard let returned else { return }
                self?.send(.signInReturned(url: returned.absoluteString))
            }
        case .copyText(let text, _):
            UIPasteboard.general.string = text
        case .clearNotification(let id):
            SaleAlerts.clear(id)
        case .haptic:
            UINotificationFeedbackGenerator().notificationOccurred(.success)
        default:
            break
        }
    }

    /// Step 4, while setup shows it; the onboarding view kept from before
    /// says nothing once the till's screens are up.
    private var posStep: PosStep? {
        guard route == .onboarding, case .pointOfSale(let view) = onboarding?.page else { return nil }
        return view
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

/// The sale on screen, as each of its screens draws it (`Update.sale`).
struct SaleScreens {
    let orderId: String
    let view: SaleView
    let page: SalePageView
    let code: CodeView
    /// Every line of its order, for the order screen.
    let order: OrderPageView?
}

/// The close sheet over a sale, while one is asked (`Update.closeSheet`).
struct CloseSheet {
    let orderId: String
    let view: CloseView
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
