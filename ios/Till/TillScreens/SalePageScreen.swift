import SwiftUI
import TillCore

/// A sale's page once its code is down (PadDetail, PadPaid, Det*; the
/// design's SaleDetail), drawn from the engine's `SalePageView`: the
/// state's hero, the heading, the amount and its line, the note, every
/// line of the order, the facts, and the page's buttons (refund at the
/// point of sale, show the code again).
struct SalePageScreen: View {
    let orderId: String
    let page: SalePageView
    let send: Send

    var body: some View {
        VStack(spacing: 0) {
            TillBar(title: page.title, line: page.titleLine, back: { send(.back) })
            ScrollView {
                VStack(spacing: 16) {
                    VStack(spacing: 8) {
                        StateBadge(icon: page.icon, ground: page.ground, size: 72)
                            .padding(.bottom, 4)
                        Text(page.heading)
                            .display(26, relativeTo: .title)
                            .foregroundStyle(page.headingWarn ? Palette.expiredInk : Palette.ink)
                            .multilineTextAlignment(.center)
                            .accessibilityAddTraits(.isHeader)
                        Text(page.amount)
                            .display(34, tracking: -0.8)
                            .foregroundStyle(page.amountMuted ? Palette.secondary : Palette.ink)
                        Text(page.line)
                            .font(.subheadline)
                            .foregroundStyle(Palette.secondary)
                            .multilineTextAlignment(.center)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.horizontal, 20)
                    .padding(.top, 12)
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
            .scrollBounceBehavior(.basedOnSize)
        }
        .foregroundStyle(Palette.ink)
        .background(Palette.ground)
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
    /// stays glass, also on a page that drops Refund (another till's sale,
    /// or one taken before the point of sale changed).
    static func primary(_ action: PageAction) -> Bool {
        switch action {
        case .refund: true
        default: false
        }
    }
}

/// A page's button: Refund the primary one, the rest glass.
struct PageButtonView: View {
    let button: PageButton
    let primary: Bool
    let action: @MainActor () -> Void

    var body: some View {
        Group {
            if primary {
                PrimaryPress(press: button.press, action: action)
            } else {
                Button {
                    action()
                } label: {
                    Text(button.press.label)
                        .frame(maxWidth: .infinity)
                }
                .glassButton()
                .disabled(!button.press.enabled)
            }
        }
        .padding(.horizontal, 16)
    }
}
