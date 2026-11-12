import SwiftUI
import TillCore

/// A sale's page once its code is down (D3 and the sale's other ends),
/// drawn from the engine's `SalePageView`: whom it's for as the toolbar's
/// title, the hero, the heading and the amount, the line, the note, every
/// line of the order, the facts, the page's buttons (refund at the point of
/// sale, show the code again), and a cash sale's (Take it in Monero, Keep
/// as cash, Paid in cash; the CashTill board).
///
/// One fact per line (round 4's F3 and F4): a 56 pt status glyph, the
/// state in its colour (Paid green, Not paid amber), the amount, and the
/// line under it; then the note, the order, the timeline, small print, the
/// buttons, and Return to Square at the foot. The system's Back leaves it.
struct SalePageScreen: View {
    let orderId: String
    let page: SalePageView
    /// The sale is paid (`isPaid`).
    let paid: Bool
    var celebrate = false
    let send: Send
    /// "Return to Square" in the toolbar, when it's offered (``BackToSquare``).
    var backToSquare: (@MainActor @Sendable () -> Void)?

    /// The tick's one moment, when this screen watched the sale turn paid.
    @State private var pop: CGFloat = 1
    @Environment(\.horizontalSizeClass) private var sizeClass

    /// The sale is paid: its payment's progress, as the engine counts it
    /// (`SaleView.progress`), is full.
    static func isPaid(_ view: SaleView) -> Bool {
        guard let progress = view.progress else { return false }
        return full(value: progress.value, total: progress.total)
    }

    /// `value` of `total` thirds is all of them: paid.
    static func full(value: UInt8, total: UInt8) -> Bool {
        total > 0 && value >= total
    }

    var body: some View {
        ScrollView {
            VStack(spacing: 16) {
                hero
                if let note = page.note {
                    NoteCard(text: note.text, ground: note.ground)
                }
                if let order = page.order {
                    OrderCardSection(card: order)
                }
                if !page.facts.isEmpty {
                    FactCard(facts: page.facts)
                }
                if let footnote = page.footnote {
                    Text(footnote)
                        .font(.footnote)
                        .foregroundStyle(Palette.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 20)
                }
                if let problem = page.refundProblem {
                    ProblemNote(text: problem)
                }
                ForEach(Array(page.buttons.enumerated()), id: \.offset) { _, button in
                    PageButtonView(button: button, primary: SalePageScreen.primary(button.action)) {
                        send(action(button.action))
                    }
                }
                CashButtons(orderId: orderId, buttons: page.cash, send: send)
                    .padding(.horizontal, 16)
            }
            .padding(.bottom, 24)
        }
        .readableWidth()
        .safeAreaInset(edge: .bottom, spacing: 0) {
            // Return to Square at the foot (round 4), as on a live sale.
            if let backToSquare, !page.backTo.isEmpty {
                ReturnToSquareWide(label: page.backTo, action: backToSquare)
                    .frame(maxWidth: readableColumn)
                    .padding(.horizontal, 16)
                    .padding(.top, 10)
                    .padding(.bottom, sizeClass == .regular ? 20 : 8)
                    .frame(maxWidth: .infinity)
                    .background(Palette.ground)
            }
        }
        .foregroundStyle(Palette.ink)
        .background(Palette.ground)
        .navigationTitle(page.title)
        .navigationSubtitle(page.titleLine)
        .toolbarTitleDisplayMode(.inline)
        .task(id: orderId) {
            guard paid, celebrate else { return }
            pop = 0.6
            withAnimation(.snappy(duration: 0.35)) { pop = 1 }
        }
    }

    /// The state's colour: Paid green, a sale that ended unpaid or needs
    /// the cashier amber, the rest ink.
    private var headingColour: Color {
        if paid || page.icon == .tick { return Palette.paid }
        if page.headingWarn || page.icon == .clock || page.ground == .warn { return Palette.expiredInk }
        return Palette.ink
    }

    private var hero: some View {
        VStack(spacing: 6) {
            StatusGlyph(kind: StatusGlyphKind.of(page.icon, step: 4), size: 56)
                .scaleEffect(pop)
            Text(page.heading)
                .font(.title3.weight(.semibold))
                .foregroundStyle(headingColour)
                .multilineTextAlignment(.center)
                .padding(.top, 4)
                .accessibilityAddTraits(.isHeader)
            Text(page.amount)
                .display(44, tracking: -1)
                .foregroundStyle(page.amountMuted ? Palette.secondary : Palette.ink)
                .lineLimit(1)
                .minimumScaleFactor(0.6)
            Text(page.line)
                .font(.subheadline)
                .foregroundStyle(Palette.secondary)
                .multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity)
        .padding(.horizontal, 20)
        .padding(.top, 16)
    }

    /// What a page button asks of the engine.
    private func action(_ action: PageAction) -> Action {
        SalePageScreen.action(action, orderId: orderId)
    }

    static func action(_ action: PageAction, orderId: String) -> Action {
        switch action {
        case .refund: .refund(orderId: orderId)
        case .showCodeAgain: .reopen(orderId: orderId)
        case .newCode: .newCode(orderId: orderId)
        case .markRefunded: .extraRefunded(orderId: orderId)
        }
    }

    /// Refund is the page's primary button, as on Android; Show code again
    /// stays secondary, also on a page that drops Refund (another till's
    /// sale, or one taken before the point of sale changed).
    static func primary(_ action: PageAction) -> Bool {
        switch action {
        case .refund, .markRefunded: true
        default: false
        }
    }
}

/// A page's button: Refund the primary one, the rest outlined.
struct PageButtonView: View {
    let button: PageButton
    let primary: Bool
    let action: @MainActor () -> Void

    var body: some View {
        Group {
            if primary {
                PrimaryPress(press: button.press, systemImage: PageButtonView.icon(button.action), action: action)
            } else {
                Button {
                    action()
                } label: {
                    Label(button.press.label, systemImage: PageButtonView.icon(button.action))
                }
                .buttonStyle(.secondary)
                .disabled(!button.press.enabled)
            }
        }
        .padding(.horizontal, 16)
    }

    /// What each button does, as an icon beside its words, on iOS as on
    /// Android (owner, round 4).
    static func icon(_ action: PageAction) -> String {
        switch action {
        case .refund: "arrow.uturn.backward"
        case .showCodeAgain, .newCode: "qrcode"
        case .markRefunded: "checkmark"
        }
    }
}
