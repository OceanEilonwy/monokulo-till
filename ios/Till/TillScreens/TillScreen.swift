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
/// On a keypad till (core.md decision 257) the list is home too, with New
/// sale at its foot, and the keypad a full-screen cover over it with Close.
/// In a wide window (a landscape iPad) the two sit side by side instead:
/// the list in a column of its own, without New sale, and beside it the
/// keypad, or the sale open from the list with New sale under it.
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
        GeometryReader { box in
            // The window's whole height, the keyboard's too, so typing a
            // reference doesn't turn a tall window into a wide one.
            let window = CGSize(width: box.size.width, height: box.size.height + box.safeAreaInsets.top + box.safeAreaInsets.bottom)
            layout(wide: keypadTill && TillScreen.sideBySide(window))
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

    // MARK: One stack, or a keypad till side by side

    /// A keypad till: the engine sends its keypad with the list.
    private var keypadTill: Bool {
        core.keypad != nil && core.shift?.newSale != nil
    }

    /// The list and the keypad side by side in a window this size: a
    /// landscape iPad, wide enough for the list's column and the keypad.
    static func sideBySide(_ size: CGSize) -> Bool {
        size.width >= 900 && size.width > size.height
    }

    /// The list's column beside the keypad.
    static let listColumn: CGFloat = 440

    @ViewBuilder
    private func layout(wide: Bool) -> some View {
        if wide {
            split
        } else {
            stack
                .fullScreenCover(isPresented: keypadUp) {
                    NavigationStack {
                        if let keypad = core.keypad {
                            KeypadScreen(view: keypad, send: send)
                        } else {
                            TillLoading()
                        }
                    }
                }
        }
    }

    /// The shift at the root at every width, a sale and its order pushed
    /// over it.
    private var stack: some View {
        NavigationStack(path: stackPath) {
            shiftScreen(sidebar: false)
                .navigationDestination(for: Route.self) { destination in
                    page(destination, newSale: false)
                }
        }
    }

    /// A keypad till in a wide window: the list in its column, Settings in
    /// its bar; beside it the keypad, with the sale open from the list
    /// pushed over it, New sale under the sale.
    private var split: some View {
        HStack(spacing: 0) {
            NavigationStack {
                shiftScreen(sidebar: true, selected: TillScreen.orderId(shown), newSale: false)
            }
            .frame(width: TillScreen.listColumn)
            Rectangle()
                .fill(Palette.border)
                .frame(width: 1)
                .ignoresSafeArea()
            NavigationStack(path: stackPath) {
                Group {
                    if let keypad = core.keypad {
                        KeypadScreen(view: keypad, closes: false, send: send)
                    } else {
                        TillLoading()
                    }
                }
                .navigationDestination(for: Route.self) { destination in
                    page(destination, newSale: true)
                }
            }
            .frame(maxWidth: .infinity)
        }
    }

    /// The page the stack pushes for `destination`; beside the list on a
    /// keypad till, with New sale under it.
    @ViewBuilder
    private func page(_ destination: Route, newSale: Bool) -> some View {
        switch destination {
        case .shift:
            shiftScreen(sidebar: false)
        case .sale(let orderId):
            SaleDetail(core: core, orderId: orderId, celebrate: lastLive == orderId, send: send)
                .modifier(NewSaleFoot(press: newSale ? core.shift?.newSale : nil, send: send))
        case .order(let orderId):
            OrderDetail(core: core, orderId: orderId, send: send)
                .modifier(NewSaleFoot(press: newSale ? core.shift?.newSale : nil, send: send))
        default:
            TillLoading()
        }
    }

    /// The keypad over the list, up while the engine shows it, on a phone
    /// or a narrow window. Only Close (the engine's Back) takes it down.
    private var keypadUp: Binding<Bool> {
        Binding(
            get: { route == .keypad },
            set: { _ in }
        )
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
    /// its order. The code is a cover over its sale, not a page, and so is
    /// the keypad over the list (or the root beside it).
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
    private func shiftScreen(sidebar: Bool, selected: String? = nil, newSale: Bool = true) -> some View {
        if let shift = core.shift {
            ShiftScreen(view: shift, sidebar: sidebar, selected: selected, newSale: newSale, send: send)
        } else {
            TillLoading()
        }
    }
}

/// New sale under a sale open beside a keypad till's list (core.md
/// decision 257): the sale stays in the list, and the keypad comes back.
struct NewSaleFoot: ViewModifier {
    let press: Press?
    let send: Send

    func body(content: Content) -> some View {
        content
            .safeAreaInset(edge: .bottom, spacing: 0) {
                if let press {
                    PrimaryPress(press: press, systemImage: "plus") { send(.newSale) }
                        .frame(maxWidth: 440)
                        .padding(.horizontal, 16)
                        .padding(.vertical, 8)
                        .frame(maxWidth: .infinity)
                        .background(Palette.ground)
                }
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
