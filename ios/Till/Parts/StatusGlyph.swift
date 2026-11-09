import SwiftUI
import TillCore

/// A sale's or the till's small status mark (the StatusGlyph board), always
/// beside a word that says the same: SF Symbols where one fits, else a
/// small shape in their weight.
enum StatusGlyphKind: Hashable, Sendable {
    case watching
    case waiting
    case seen
    case paid
    case closed
    case cash
    /// A reversed payment: don't hand it over.
    case reversed
    /// Refunded, or to refund at the point of sale.
    case refund
    /// Another till's sale, closed there.
    case otherTill

    /// The SF Symbol drawn; nil for watching, which is drawn.
    var symbol: String? {
        switch self {
        case .watching: nil
        case .waiting: "clock"
        case .seen: "circle.righthalf.filled"
        case .paid: "checkmark.circle.fill"
        case .closed: "exclamationmark.circle"
        case .cash: "banknote"
        case .reversed: "exclamationmark.triangle.fill"
        case .refund: "arrow.uturn.backward.circle"
        case .otherTill: "xmark.circle"
        }
    }

    /// Its colour: progress orange seen, green only paid, warn closed.
    var colour: Color {
        switch self {
        case .watching, .waiting, .cash, .refund, .otherTill: Palette.secondary
        case .seen: Palette.progress
        case .paid: Palette.paid
        case .closed: Palette.expiredInk
        case .reversed: Palette.danger
        }
    }

    /// The glyph for a shift row's state as the engine draws it: a live
    /// sale's ring at its step, waiting until the payment is seen.
    static func of(_ icon: StateIcon, step: Int) -> StatusGlyphKind {
        switch icon {
        case .ring: step >= 2 ? .seen : .waiting
        case .clock: .closed
        case .warning: .reversed
        case .tick: .paid
        case .refund: .refund
        case .close: .otherTill
        case .cash: .cash
        }
    }
}

/// A status glyph at `size` points.
struct StatusGlyph: View {
    let kind: StatusGlyphKind
    var size: CGFloat = 24

    var body: some View {
        Group {
            if let symbol = kind.symbol {
                Image(systemName: symbol)
                    .resizable()
                    .scaledToFit()
                    .fontWeight(.medium)
                    .padding(size * 0.06)
            } else {
                WatchingShape()
                    .stroke(Palette.secondary, style: StrokeStyle(lineWidth: size * 1.8 / 24, lineCap: .round))
                    .overlay {
                        Circle()
                            .fill(Palette.progress)
                            .frame(width: size / 4, height: size / 4)
                    }
            }
        }
        .foregroundStyle(kind.colour)
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}

/// Watching's two arcs either side of the dot (the board's
/// `M7.1 7.1a7 7 0 0 0 0 9.8M16.9 7.1a7 7 0 0 1 0 9.8`, in 24 units).
struct WatchingShape: Shape {
    func path(in rect: CGRect) -> Path {
        let unit = min(rect.width, rect.height) / 24
        let centre = CGPoint(x: rect.midX, y: rect.midY)
        var path = Path()
        path.addArc(center: centre, radius: 7 * unit, startAngle: .degrees(135), endAngle: .degrees(225), clockwise: false)
        path.move(to: CGPoint(x: centre.x + 7 * unit * cos(.pi / 4), y: centre.y - 7 * unit * sin(.pi / 4)))
        path.addArc(center: centre, radius: 7 * unit, startAngle: .degrees(-45), endAngle: .degrees(45), clockwise: false)
        return path
    }
}
