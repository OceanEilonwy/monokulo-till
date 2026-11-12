import Foundation
import Observation
import os
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
    /// Of those, what no screen's own view shows, for the root to show
    /// (docs/decisions/core.md 109).
    private(set) var shownBusy = false
    private(set) var shownProblem: String?
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
    /// The engine's half of going to the background and back, and the
    /// extension's word that it wrote the ledger (CoreLifecycle.swift).
    private let lifecycle = EngineLifecycle()
    private var ledgerSignal: LedgerSignalObserver?
    private let signIn: SignInSession
    /// Taps on a new sale's alert, which open the sale.
    private var taps: NotificationTaps?
    /// iOS's memory warnings (`memoryWarning`).
    @ObservationIgnored private var memoryWarnings: (any NSObjectProtocol)?
    /// The iPhone's Live Activity (`Update.liveActivity`). Its push token
    /// goes to `registerLiveActivityPushToken` (phase I5).
    let liveActivity = LiveActivityController()

    init(secrets: Platform = DevicePlatform(), push: PushPlatform = ApnsPush()) {
        config = Core.config()
        signIn = SignInSession()
        let listener = Listener(core: self)
        // Before the engine's first write to the ledger.
        setBackgroundWork(work: DeviceBackgroundWork.shared)
        engine = Engine(config: config, platform: secrets, push: push, listener: listener)
        let taps = NotificationTaps(
            open: { [weak self] orderId in
                self?.send(.openSale(orderId: orderId))
            },
            relayed: { [weak self] ciphertext in
                // A Bell's ding the relay carried while Till is in front:
                // the core opens it, and a ding wakes the shift.
                self?.send(.pushReceived(content: ciphertext, decrypted: false))
            }
        )
        self.taps = taps
        ledgerSignal = LedgerSignalObserver { [weak engine = self.engine] in engine?.ledgerChanged() }
        liveActivity.onPushToken = { [weak self] token in
            self?.registerLiveActivityPushToken(token)
        }
        UNUserNotificationCenter.current().delegate = taps
        memoryWarnings = NotificationCenter.default.addObserver(
            forName: UIApplication.didReceiveMemoryWarningNotification, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.memoryWarning() }
        }
    }

    /// iOS is short of memory: what can be made again goes, and the log
    /// says how close the app is to being ended.
    private func memoryWarning() {
        let left = MemoryHeadroom.described
        TillLog.memory.error("memory warning: \(left, privacy: .public) left")
        QRImages.empty()
    }

    /// How a new sale shows up on this iPad, as Settings has it.
    var showSales: ShowSales {
        settings?.showSales?.choices.first { $0.choice.selected }?.mode ?? .alert
    }

    /// Which screen draws the app now.
    var route: Route { Route.of(screen) }

    func send(_ action: Action) {
        engine?.send(action: action)
    }

    /// A link that opened the app: the Live Activity's tap, a connect
    /// link, or a sign-in coming back.
    func open(_ url: URL) {
        if TillActivityLink.isSale(url) {
            // Without an order (the state came by push) it only brings Till forward.
            if let orderId = TillActivityLink.orderId(url) {
                send(.openSale(orderId: orderId))
            }
            return
        }
        switch IncomingLink.of(url, square: config.square.redirectUri, clover: config.clover.redirectUri) {
        case .connect(let link): send(.connectLink(url: link))
        case .signInReturned(let link): send(.signInReturned(url: link))
        }
    }

    /// The app came to the front: the engine publishes everything again,
    /// and hears what the system allows now.
    func opened() {
        send(.opened)
        if let engine { lifecycle.resume(engine) }
        liveActivity.resume()
        Task { await readPermissions() }
    }

    /// The app went to the background: the engine hears it, then closes
    /// its ledger before iOS may suspend the app (`Engine.suspend`).
    func backgrounded() {
        let left = MemoryHeadroom.described
        TillLog.lifecycle.notice("to the back: memory left \(left, privacy: .public)")
        send(.backgrounded)
        if let engine { lifecycle.suspend(engine) }
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
        case .work(let busy, let problem, let shownBusy, let shownProblem):
            self.shownBusy = shownBusy
            self.shownProblem = shownProblem
            self.busy = busy
            self.problem = problem
        case .nav(let byItself, _, _, _, _): self.byItself = byItself
        case .shift(let view):
            shift = view
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
        case .liveActivity(let state):
            liveActivity.show(
                state.map { TillActivityAttributes.ContentState($0) },
                attributes: TillActivityAttributes(store: settings?.store?.name, place: shift?.top.place)
            )
        case .notification(let view):
            // Set in one update only, once the engine has claimed the
            // sale's alert (core.md 123), as it arrives with Till in the
            // back.
            if let alert = view.alert {
                Task { await SaleAlerts.post(alert) }
            }
        default: break
        }
        // The update's name only: `String(describing:)` of the whole update
        // reflected over every sale of a shift each second the countdown
        // ran, on the main thread, and kept 50 such dumps, so the app's
        // memory and work grew with the shift (the 2026-11-10 crash).
        updates.append(Self.name(of: update))
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
        case .notify(let orderId, let notice, _, _, let quiet):
            // Paid and "No payment yet" while Till is behind: on iPad and on
            // an iPhone without the Live Activity (the engine decides).
            if !quiet {
                Task { await SaleAlerts.notice(orderId: orderId, notice) }
            }
        case .probeTor, .lowerPanel:
            // No Tor and no bubble on iOS: the engine never sends these here.
            break
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

    /// An update's case, without what it carries.
    private static func name(of update: Update) -> String {
        Mirror(reflecting: update).children.first?.label ?? String(describing: update)
    }

    /// What the engine needs to know about this device and build
    /// (`EngineConfig`): the extension's own, but for this device's name
    /// (`TillShared.config`), so both keep their files in the App Group's
    /// container and push through the relay.
    private static func config() -> EngineConfig {
        let device = UIDevice.current
        return TillShared.config(
            device: device.userInterfaceIdiom == .pad ? .ipad : .iphone,
            deviceName: device.name
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
