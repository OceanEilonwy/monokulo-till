import SwiftUI
import TillCore

/// The widest a page's content grows. Pages scroll across the whole
/// window; only their content is held to this column, by the scroll view's
/// content margins (docs/ipad-redesign.md, decision 24).
let readableColumn: CGFloat = 560

extension View {
    /// Holds a scroll view's or list's content to `width` in the middle of
    /// a wider window, while the whole window still scrolls.
    func readableWidth(_ width: CGFloat = readableColumn) -> some View {
        modifier(ReadableMargins(maxWidth: width))
    }
}

/// The content margins that centre a column of `maxWidth` in the scroll
/// view's own width.
struct ReadableMargins: ViewModifier {
    let maxWidth: CGFloat

    @State private var width: CGFloat = 0

    func body(content: Content) -> some View {
        content
            .contentMargins(.horizontal, ReadableMargins.margin(width: width, maxWidth: maxWidth), for: .scrollContent)
            .onGeometryChange(for: CGFloat.self) { proxy in
                proxy.size.width
            } action: { newWidth in
                width = newWidth
            }
    }

    /// Each side's margin: half of what the window has beyond the column.
    static func margin(width: CGFloat, maxWidth: CGFloat) -> CGFloat {
        max(0, (width - maxWidth) / 2)
    }
}

/// How far along a payment is, as the Till logo shows it: nothing while
/// it waits, a third seen, two thirds confirming, all of it paid.
enum PaymentSteps {
    /// The bar's fraction at the engine's `step` (1 waiting … 4 paid).
    static func fraction(step: Int) -> Double {
        switch step {
        case ...1: 0
        case 2: 1.0 / 3
        case 3: 2.0 / 3
        default: 1
        }
    }
}

/// A live sale's progress: the system's linear `ProgressView`, tinted
/// the progress orange (green once full, paid), with its label over the
/// bar, the engine's words ("Seen on the network") at the start and the
/// count ("1 of 3") at the end, and the line under it ("Scan with any
/// Monero wallet · 14:52 left"). The bar is at the engine's `fraction`
/// where it gives one (`SaleView.progress`), else at the ring's step.
/// Each line stays one line: shrunk a little, then cut short
/// (docs/ipad-redesign.md, decisions 17, 22 and 29).
struct PaymentProgress: View {
    let step: Int
    let status: String
    var count: String? = nil
    var hint: String? = nil
    var fraction: Double? = nil
    /// What a screen reader says for the bar.
    var said: String? = nil

    var body: some View {
        let value = min(1, max(0, fraction ?? PaymentSteps.fraction(step: step)))
        ProgressView(value: value) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(status)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(Palette.ink)
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                if let count, !count.isEmpty {
                    Text(count)
                        .font(.subheadline)
                        .foregroundStyle(Palette.secondary)
                        .monospacedDigit()
                        .lineLimit(1)
                        .fixedSize()
                }
            }
        } currentValueLabel: {
            if let hint, !hint.isEmpty {
                Text(hint)
                    .font(.subheadline)
                    .foregroundStyle(Palette.secondary)
                    .monospacedDigit()
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
            }
        }
        .progressViewStyle(.linear)
        .tint(value >= 1 ? Palette.paid : Palette.progress)
        .animation(.snappy(duration: 0.35), value: value)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("Payment")
        .accessibilityValue(said ?? [status, count ?? ""].filter { !$0.isEmpty }.joined(separator: ", "))
    }
}

/// What was bought (the design's order card): the receipt's summary, its
/// lines, "+N more" (which opens every line), and the foot and total.
struct OrderCardSection: View {
    let card: OrderCard
    var more: (@MainActor () -> Void)? = nil

    var body: some View {
        Card {
            VStack(alignment: .leading, spacing: 8) {
                Text(card.summary)
                    .font(.footnote)
                    .foregroundStyle(Palette.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                ForEach(Array(card.lines.enumerated()), id: \.offset) { _, line in
                    HStack(alignment: .firstTextBaseline, spacing: 12) {
                        Text(line.label)
                            .fixedSize(horizontal: false, vertical: true)
                        Spacer(minLength: 8)
                        Text(line.price)
                            .monospacedDigit()
                            .fixedSize()
                    }
                    .font(.subheadline)
                    .accessibilityElement(children: .combine)
                }
                if let label = card.more {
                    if let more {
                        Button(label) { more() }
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(Palette.link)
                            .buttonStyle(.plain)
                            .frame(minHeight: minimumTarget)
                    } else {
                        Text(label)
                            .font(.subheadline)
                            .foregroundStyle(Palette.secondary)
                    }
                }
                Rectangle()
                    .fill(Palette.hairline)
                    .frame(height: 1)
                HStack(alignment: .firstTextBaseline) {
                    Text(card.foot)
                        .foregroundStyle(Palette.secondary)
                    Spacer(minLength: 8)
                    Text(card.total)
                        .fontWeight(.semibold)
                        .monospacedDigit()
                        .fixedSize()
                }
                .font(.subheadline)
                .accessibilityElement(children: .combine)
            }
            .padding(16)
        }
    }
}

/// A note on its ground: a page's few sentences.
struct NoteCard: View {
    let text: String
    let ground: Ground

    var body: some View {
        Text(text)
            .font(.subheadline)
            .foregroundStyle(Roles.onGround(ground))
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(14)
            .background(RoundedRectangle(cornerRadius: 12).fill(Roles.ground(ground)))
            .padding(.horizontal, 16)
    }
}

/// The engine's payment progress (`SaleView.progress`) as the bar draws
/// it: how full, in thirds. Its words are the engine's one line
/// ("Confirming · 4 of 10 confirmations", core.md decision 129).
enum EngineProgress {
    static func fraction(value: UInt8, total: UInt8) -> Double {
        guard total > 0 else { return 0 }
        return min(1, Double(value) / Double(total))
    }

    /// What stands at the end of the bar's label: the engine's line after
    /// its words ("4 of 10 confirmations" from "Confirming · 4 of 10
    /// confirmations"), else its count ("1 of 3").
    static func count(label: String, count: String, line: String) -> String {
        let lead = label + " · "
        if !label.isEmpty, line.hasPrefix(lead), line.count > lead.count {
            return String(line.dropFirst(lead.count))
        }
        return count
    }
}

/// A cash sale's buttons (`CashButton`; docs/ipad-redesign.md, decisions 7
/// and 8): Take it in Monero the primary one, Keep as cash and Paid in
/// cash outlined, side by side where they fit.
struct CashButtons: View {
    let orderId: String
    let buttons: [CashButton]
    let send: Send

    var body: some View {
        if !buttons.isEmpty {
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 12) { items }
                VStack(spacing: 10) { items }
            }
        }
    }

    private var items: some View {
        ForEach(Array(buttons.enumerated()), id: \.offset) { _, button in
            if CashButtons.primary(button.action) {
                PrimaryPress(press: button.press) { send(CashButtons.action(button.action, orderId: orderId)) }
            } else {
                Button(button.press.label) { send(CashButtons.action(button.action, orderId: orderId)) }
                    .buttonStyle(.secondary)
                    .disabled(!button.press.enabled)
            }
        }
    }

    /// What a cash button asks of the engine.
    static func action(_ action: CashAction, orderId: String) -> Action {
        switch action {
        case .takeInMonero: .takeInMonero(orderId: orderId)
        case .keepAsCash: .keepAsCash(orderId: orderId)
        case .paidInCash: .paidInCash(orderId: orderId)
        }
    }

    static func primary(_ action: CashAction) -> Bool {
        action == .takeInMonero
    }
}

/// A screen waiting for the engine's first record of it.
struct TillLoading: View {
    var body: some View {
        ProgressView()
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .accessibilityLabel("Loading")
    }
}
