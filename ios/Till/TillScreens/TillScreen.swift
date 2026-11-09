import SwiftUI
import TillCore

/// The till (phase I3, laid out again by docs/ipad-redesign.md, decisions
/// 15–19 and 23–24), once setup is done, drawn from the engine's records.
/// The engine's screen is the source of truth; the system's navigation
/// only shows it:
///
/// - compact width (iPhone, a narrow iPad window): a `NavigationStack`
///   whose path is the engine's screen, so the edge swipe goes back (D2);
/// - regular width with sales: a `NavigationSplitView`, the shift's sales
///   on the left and the open sale on the right (D1, E2);
/// - the empty till, at any width: one centred block (E1).
///
/// The customer's code is a full-screen cover, Settings a sheet with Done,
/// and the close and refund sheets go up while the engine has them up.
/// Layout follows the size class, never the device.
struct TillScreen: View {
    let core: Core
    let route: Route

    @Environment(\.horizontalSizeClass) private var sizeClass

    /// The sheets, up while the engine has them up.
    @State private var closeUp = false
    @State private var refundUp = false
    /// What shows under Settings' sheet: the screen it was opened from.
    @State private var under: Route = .shift
    /// The last sale shown while it was live: its Paid is the one moment.
    @State private var lastLive: String?

    var body: some View {
        Group {
            if sizeClass == .regular && hasSales {
                split
            } else {
                stack
            }
        }
        .background(Palette.ground)
        .sheet(isPresented: settingsUp) {
            if let settings = core.settings {
                SettingsScreen(view: settings, wakeUps: core.wakeUps, send: send)
            }
        }
        // Dismissed by a swipe, a sheet went without an answer.
        .sheet(isPresented: $closeUp, onDismiss: { if core.closeSheet != nil { core.send(.dismissClose) } }) {
            if let sheet = core.closeSheet {
                CloseSheetScreen(sheet: sheet, send: send)
                    .presentationDetents([.medium, .large])
                    .presentationDragIndicator(.visible)
                    .presentationBackground(Palette.ground)
            }
        }
        .sheet(isPresented: $refundUp, onDismiss: { if core.refundSheet != nil { core.send(.dismissRefund) } }) {
            if let view = core.refundSheet {
                RefundSheetScreen(view: view, send: send)
                    .presentationDetents([.medium, .large])
                    .presentationDragIndicator(.visible)
                    .presentationBackground(Palette.ground)
            }
        }
        .fullScreenCover(isPresented: codeUp) {
            if let sale = core.sale {
                CodeScreen(view: sale.code, send: send, phone: sizeClass != .regular)
            }
        }
        .onChange(of: core.closeSheet != nil, initial: true) { _, up in closeUp = up }
        .onChange(of: core.refundSheet != nil, initial: true) { _, up in refundUp = up }
        .onChange(of: route, initial: true) { _, now in
            if now != .settings { under = now }
        }
        .onChange(of: liveSale, initial: true) { _, live in
            if let live { lastLive = live }
        }
    }

    /// The cashier's actions, to the engine.
    private var send: Send {
        { action in core.send(action) }
    }

    /// The screen the till's navigation shows: under Settings, what it was
    /// opened from.
    private var shown: Route {
        route == .settings ? under : route
    }

    /// The sale on screen while its code is live.
    private var liveSale: String? {
        guard let orderId = TillScreen.orderId(route), let sale = core.sale, sale.orderId == orderId, !sale.code.done else {
            return nil
        }
        return orderId
    }

    /// The shift has had a sale (the engine's chips come with the first).
    private var hasSales: Bool {
        guard let shift = core.shift, shift.disconnected == nil else { return false }
        return !shift.chips.isEmpty || !shift.entries.isEmpty
    }

    private var settingsUp: Binding<Bool> {
        Binding(
            get: { route == .settings },
            set: { up in
                // Swiped down: Settings goes back as Done would.
                if !up && route == .settings { core.send(.back) }
            }
        )
    }

    /// The customer's code: up while the engine shows it and the sale is
    /// live; once paid it gives way to the sale's page underneath.
    private var codeUp: Binding<Bool> {
        Binding(
            get: {
                guard case .code(let orderId) = route, let sale = core.sale, sale.orderId == orderId else { return false }
                return !sale.code.done
            },
            set: { _ in }
        )
    }

    // MARK: Compact: one stack

    private var stack: some View {
        NavigationStack(path: stackPath) {
            shiftScreen(sidebar: false)
                .navigationDestination(for: Route.self) { destination in
                    switch destination {
                    case .sale(let orderId):
                        SaleDetail(core: core, orderId: orderId, celebrate: lastLive == orderId, send: send)
                    case .order(let orderId):
                        OrderDetail(core: core, orderId: orderId, send: send)
                    default:
                        TillLoading()
                    }
                }
        }
    }

    /// The stack's path, from the engine's screen; a swipe or Back that
    /// pops it tells the engine as many Backs.
    private var stackPath: Binding<[Route]> {
        Binding(
            get: { TillScreen.stackPath(shown) },
            set: { path in
                guard route != .settings else { return }
                for _ in 0..<TillScreen.backs(from: route, toDepth: path.count) {
                    core.send(.back)
                }
            }
        )
    }

    /// The stack for `route`: the shift at its root, then the sale, then
    /// its order. The code is a cover over its sale, not a page.
    static func stackPath(_ route: Route) -> [Route] {
        switch route {
        case .sale(let orderId), .code(let orderId):
            [.sale(orderId: orderId)]
        case .order(let orderId):
            [.sale(orderId: orderId), .order(orderId: orderId)]
        default:
            []
        }
    }

    /// How deep the engine holds `route`: the shift, a sale, and a sale's
    /// code or order under it (its Back goes up one).
    static func depth(_ route: Route) -> Int {
        switch route {
        case .sale: 1
        case .code, .order: 2
        default: 0
        }
    }

    /// How many Backs bring the engine from `route` to a stack `depth` deep.
    static func backs(from route: Route, toDepth depth: Int) -> Int {
        max(0, TillScreen.depth(route) - depth)
    }

    // MARK: Regular: sales beside the sale

    private var split: some View {
        NavigationSplitView {
            shiftScreen(sidebar: true)
                .navigationSplitViewColumnWidth(min: 300, ideal: 340, max: 420)
        } detail: {
            NavigationStack(path: detailPath) {
                detailRoot
                    .navigationDestination(for: Route.self) { destination in
                        if case .order(let orderId) = destination {
                            OrderDetail(core: core, orderId: orderId, send: send)
                        } else {
                            TillLoading()
                        }
                    }
            }
        }
        .navigationSplitViewStyle(.balanced)
    }

    /// The detail column's root: the open sale, or the empty till.
    @ViewBuilder
    private var detailRoot: some View {
        if let orderId = TillScreen.orderId(shown) {
            SaleDetail(core: core, orderId: orderId, celebrate: lastLive == orderId, send: send)
                .id(orderId)
        } else if let shift = core.shift {
            EmptyTill(idle: shift.idle, chips: statusChips, send: send)
                .toolbar(.hidden, for: .navigationBar)
        } else {
            TillLoading()
        }
    }

    private var detailPath: Binding<[Route]> {
        Binding(
            get: {
                if case .order(let orderId) = shown { return [.order(orderId: orderId)] }
                return []
            },
            set: { path in
                if path.isEmpty, case .order = route { core.send(.back) }
            }
        )
    }

    /// The sale `route` shows, if it shows one.
    static func orderId(_ route: Route) -> String? {
        switch route {
        case .sale(let orderId), .code(let orderId), .order(let orderId): orderId
        default: nil
        }
    }

    // MARK: The shift

    @ViewBuilder
    private func shiftScreen(sidebar: Bool) -> some View {
        if let shift = core.shift {
            ShiftScreen(
                view: shift,
                chips: statusChips,
                sidebar: sidebar,
                selected: sidebar ? TillScreen.orderId(shown) : nil,
                send: send
            )
        } else {
            TillLoading()
        }
    }

    /// The empty till's status chips, from what the engine publishes now.
    private var statusChips: [StatusChip] {
        StatusChip.chips(
            top: core.shift?.top,
            alerts: core.settings?.alerts.on,
            bell: core.wakeUps.map { $0.monokuloBell.selected || $0.ownBell.selected }
        )
    }
}

/// One sale, as the engine has it now: its code while it's live, else its
/// page (Paid among them).
struct SaleDetail: View {
    let core: Core
    let orderId: String
    let celebrate: Bool
    let send: Send

    var body: some View {
        if let sale = core.sale, sale.orderId == orderId {
            if sale.code.done {
                SalePageScreen(orderId: orderId, page: sale.page, celebrate: celebrate, send: send)
            } else {
                SaleScreen(orderId: orderId, view: sale.view, send: send)
            }
        } else {
            TillLoading()
        }
    }
}

/// Every line of a sale's order, as the engine has it now.
struct OrderDetail: View {
    let core: Core
    let orderId: String
    let send: Send

    var body: some View {
        if let sale = core.sale, sale.orderId == orderId {
            OrderScreen(title: sale.view.title, order: sale.order, send: send)
        } else {
            TillLoading()
        }
    }
}
