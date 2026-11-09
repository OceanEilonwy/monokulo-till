import SwiftUI
import TillCore

/// A sale's page once its code is down (D3 and the sale's other ends),
/// drawn from the engine's `SalePageView`: whom it's for as the toolbar's
/// title, the hero, the heading and the amount, the line, the note, every
/// line of the order, the facts, and the page's buttons (refund at the
/// point of sale, show the code again).
///
/// Paid (D3) has the Till logo, green with its tick, as its hero, "Paid ·
/// $13.88" and Done. A sale this screen watched turn paid gets the one
/// moment: the logo's bar fills and it turns green (a crossfade under
/// Reduce Motion); the engine's paid haptic plays with it.
struct SalePageScreen: View {
    let orderId: String
    let page: SalePageView
    var celebrate = false
    let send: Send
    /// Paid's "Back to Square" (A3), when it's offered (``BackToSquare``).
    var backToSquare: (@MainActor () -> Void)?

    @State private var logo: TillLogoState = .paid

    /// The page is a paid sale's.
    static func isPaid(_ page: SalePageView) -> Bool {
        page.icon == .tick
    }

    private var paid: Bool { SalePageScreen.isPaid(page) }

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
                if let problem = page.refundProblem {
                    ProblemNote(text: problem)
                }
                ForEach(Array(page.buttons.enumerated()), id: \.offset) { _, button in
                    PageButtonView(button: button, primary: SalePageScreen.primary(button.action)) {
                        send(action(button.action))
                    }
                }
            }
            .padding(.bottom, 24)
        }
        .readableWidth()
        .safeAreaInset(edge: .bottom) {
            if paid {
                VStack(spacing: 10) {
                    if let backToSquare {
                        Button {
                            backToSquare()
                        } label: {
                            Label("Back to Square", systemImage: "chevron.left")
                        }
                        .buttonStyle(.primary)
                        .accessibilityHint("Opens Square Point of Sale")
                    }
                    Button("Done") { send(.back) }
                        .buttonStyle(.secondary)
                }
                .frame(maxWidth: 440)
                .padding(.horizontal, 20)
                .padding(.vertical, 8)
                .frame(maxWidth: .infinity)
                .background(Palette.ground)
            }
        }
        .foregroundStyle(Palette.ink)
        .background(Palette.ground)
        .navigationTitle(page.title)
        .navigationSubtitle(page.titleLine)
        .navigationBarTitleDisplayMode(.inline)
        .task(id: orderId) {
            guard paid, celebrate else { return }
            logo = .confirming
            try? await Task.sleep(for: .milliseconds(150))
            logo = .paid
        }
    }

    @ViewBuilder
    private var hero: some View {
        VStack(spacing: 14) {
            if paid {
                TillWidget(state: logo, size: 160)
                Text("\(page.heading) · \(page.amount)")
                    .display(34, tracking: -0.8)
                    .multilineTextAlignment(.center)
                    .accessibilityAddTraits(.isHeader)
            } else {
                StatusGlyph(kind: StatusGlyphKind.of(page.icon, step: 4), size: 56)
                Text(page.heading)
                    .font(.title2.weight(.bold))
                    .foregroundStyle(page.headingWarn ? Palette.expiredInk : Palette.ink)
                    .multilineTextAlignment(.center)
                    .accessibilityAddTraits(.isHeader)
                Text(page.amount)
                    .display(34, tracking: -0.8)
                    .foregroundStyle(page.amountMuted ? Palette.secondary : Palette.ink)
                    .lineLimit(1)
                    .minimumScaleFactor(0.6)
            }
            Text(page.line)
                .font(.body)
                .foregroundStyle(Palette.secondary)
                .multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity)
        .padding(.horizontal, 20)
        .padding(.top, paid ? 32 : 12)
    }

    /// What a page button asks of the engine.
    private func action(_ action: PageAction) -> Action {
        SalePageScreen.action(action, orderId: orderId)
    }

    static func action(_ action: PageAction, orderId: String) -> Action {
        switch action {
        case .refund: .refund(orderId: orderId)
        case .showCodeAgain: .reopen(orderId: orderId)
        }
    }

    /// Refund is the page's primary button, as on Android; Show code again
    /// stays secondary, also on a page that drops Refund (another till's
    /// sale, or one taken before the point of sale changed).
    static func primary(_ action: PageAction) -> Bool {
        switch action {
        case .refund: true
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
                PrimaryPress(press: button.press, action: action)
            } else {
                Button(button.press.label) { action() }
                    .buttonStyle(.secondary)
                    .disabled(!button.press.enabled)
            }
        }
        .padding(.horizontal, 16)
    }
}
