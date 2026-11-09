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

/// A thin progress bar in the payment's colours: the progress orange on a
/// track that meets 3:1, green once full (paid). It is the system's
/// `ProgressView`, only drawn in Till's colours, so VoiceOver reads it as
/// one.
struct PaymentBarStyle: ProgressViewStyle {
    func makeBody(configuration: Configuration) -> some View {
        let fraction = min(1, max(0, configuration.fractionCompleted ?? 0))
        Capsule()
            .fill(Palette.track)
            .frame(height: 4)
            .overlay(alignment: .leading) {
                GeometryReader { box in
                    Capsule()
                        .fill(fraction >= 1 ? Palette.paid : Palette.progress)
                        .frame(width: box.size.width * fraction)
                }
            }
            .animation(.snappy(duration: 0.35), value: fraction)
    }
}

/// A live sale's progress (the boards' bar and its label): the bar, the
/// engine's status ("Seen on the network") and its line under it ("Scan
/// with any Monero wallet · 14:52 left").
struct PaymentProgress: View {
    let step: Int
    let status: String
    var hint: String? = nil

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            ProgressView(value: PaymentSteps.fraction(step: step))
                .progressViewStyle(PaymentBarStyle())
                .accessibilityLabel("Payment")
                .accessibilityValue(status)
            VStack(alignment: .leading, spacing: 2) {
                Text(status)
                    .font(.subheadline.weight(.semibold))
                    .lineLimit(1)
                if let hint, !hint.isEmpty {
                    Text(hint)
                        .font(.subheadline)
                        .foregroundStyle(Palette.secondary)
                        .monospacedDigit()
                        .lineLimit(1)
                }
            }
            .accessibilityElement(children: .combine)
        }
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

/// A screen waiting for the engine's first record of it.
struct TillLoading: View {
    var body: some View {
        ProgressView()
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .accessibilityLabel("Loading")
    }
}
