import SwiftUI
import TillCore

/// The till (phase I3; one column since the owner's round 4, decision
/// 227), once setup is done, drawn from the engine's records. The engine's
/// screen is the source of truth; the system's navigation only shows it: a
/// `NavigationStack` at every width, whose path is the engine's screen, so
/// the edge swipe goes back. The shift is one centred column on iPhone and
/// iPad alike, and a sale opens over the whole screen, as on Android; there
/// is no sidebar.
///
/// The customer's code is a full-screen cover, Settings a sheet with Done,
/// and the close and refund sheets go up while the engine has them up.
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
        stack
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

    // MARK: One stack, at every width

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
            ShiftScreen(view: shift, sidebar: false, selected: nil, send: send)
        } else {
            TillLoading()
        }
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
                SalePageScreen(
                    orderId: orderId, page: sale.page, paid: SalePageScreen.isPaid(sale.view),
                    celebrate: celebrate, send: send,
                    backToSquare: backToSquare
                )
            } else {
                SaleScreen(orderId: orderId, view: sale.view, send: send, returnToSquare: backToSquare)
            }
        } else {
            TillLoading()
        }
    }

    /// Return to Square in the toolbar of every state, when it's offered.
    private var backToSquare: (@MainActor @Sendable () -> Void)? {
        guard BackToSquare.shown(core) else { return nil }
        let core = core
        return { @MainActor @Sendable in BackToSquare.open(core) }
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
