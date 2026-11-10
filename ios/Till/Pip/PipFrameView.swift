import SwiftUI

/// One frame of the Picture in Picture window (boards P1–P4, K1), drawn at
/// the window's size in points, edge to edge: iOS draws the window's
/// corners, shadow, ✕ (top left), ⤢ (top right) and the playback control
/// over it, so nothing of Till's goes in the top corners and there are no
/// buttons of Till's own (decision 23). Sizes follow the window's, as
/// people pinch it; Dynamic Type has no say in a picture.
struct PipFrameView: View {
    let picture: PipPicture
    let size: CGSize
    /// The time the frame is drawn at, for the countdown.
    let now: Date

    var body: some View {
        Group {
            switch picture {
            case .watching(let place):
                PipStrip(size: size, ground: PipColours.strip) {
                    TillWidget(state: .idle, size: size.height * 0.62, decorative: true)
                } words: {
                    // One word: no place, to keep the strip narrow.
                    PipStripWords(title: "Watching", line: nil, size: size)
                }
            case .paid(let amount, let name):
                PipStrip(size: size, ground: PipColours.strip) {
                    TillWidget(state: .paid, size: size.height * 0.62, decorative: true)
                } words: {
                    // "Paid" over the amount; no name, to keep it narrow.
                    PipStripWords(title: "Paid", line: amount.isEmpty ? nil : amount, size: size)
                }
            case .cash(let offer):
                PipStrip(size: size, ground: PipColours.strip) {
                    StatusGlyph(kind: .cash, size: size.height * 0.44)
                } words: {
                    PipStripWords(title: offer.title, line: offer.line, size: size)
                }
            case .sale(let sale) where sale.code == nil:
                // Seen or confirming: the strip, "Confirming" over the amount.
                PipStrip(size: size, ground: PipColours.strip) {
                    TillWidget(state: .confirming, size: size.height * 0.62, decorative: true)
                } words: {
                    PipStripWords(title: sale.progressWord, line: sale.amount.isEmpty ? nil : sale.amount, size: size)
                }
            case .sale(let sale):
                PipSquare(sale: sale, side: min(size.width, size.height), now: now)
            }
        }
        .frame(width: size.width, height: size.height)
        // The strip is the dark terminal's (P1); the logo reads its scheme.
        .environment(\.colorScheme, picture.shape == .strip ? .dark : .light)
    }
}

/// The window's colours, fixed: a picture has no light or dark mode.
enum PipColours {
    /// The strip's ground, the palette's ink.
    static let strip = Color(hex: 0x1A1917)
    static let stripTitle = Color.white
    static let stripLine = Color(hex: 0xD9D4C9)
    static let square = Color.white
    static let squareInk = Color(hex: 0x1A1917)
    static let squareMuted = Color(hex: 0x5A564C)
}

/// The strip (P1, P3, K1): a mark on the left, two lines beside it, centred
/// as a group, hugging the content.
private struct PipStrip<Mark: View, Words: View>: View {
    let size: CGSize
    let ground: Color
    @ViewBuilder let mark: Mark
    @ViewBuilder let words: Words

    var body: some View {
        HStack(spacing: size.height * 0.16) {
            mark
            words
        }
        .padding(.horizontal, size.height * 0.3)
        .frame(width: size.width, height: size.height)
        .background(ground)
    }
}

/// The strip's two lines, one line each, ending in an ellipsis (decision 22).
private struct PipStripWords: View {
    let title: String
    let line: String?
    let size: CGSize

    var body: some View {
        VStack(alignment: .leading, spacing: size.height * 0.03) {
            Text(title)
                .font(.system(size: size.height * 0.27, weight: .bold))
                .foregroundStyle(PipColours.stripTitle)
            if let line {
                Text(line)
                    .font(.system(size: size.height * 0.2))
                    .foregroundStyle(PipColours.stripLine)
            }
        }
        .lineLimit(1)
        .truncationMode(.tail)
    }
}

/// The square (P2): whom it's for, the code, the amount and the time left.
/// Too small to scan, the amount and "Tap ⤢ to show the customer"; once a
/// payment is seen, how far it is.
private struct PipSquare: View {
    let sale: PipSale
    let side: CGFloat
    let now: Date

    var body: some View {
        VStack(spacing: side * 0.02) {
            if let code = sale.code, PipShape.codeFits(width: side) {
                name
                QRCodeView(text: code, label: "Payment code", size: side * 0.56, padding: side * 0.02, cornerRadius: 0)
                amountLine
            } else if sale.code != nil {
                amount(size: side * 0.16)
                Text("Tap ⤢ to show the customer")
                    .font(.system(size: side * 0.075))
                    .foregroundStyle(PipColours.squareMuted)
                    .multilineTextAlignment(.center)
                    .lineLimit(2)
            } else {
                name
                StatusGlyph(kind: .seen, size: side * 0.22)
                    .padding(.vertical, side * 0.03)
                amount(size: side * 0.1)
                if let progress = sale.progress {
                    Text(progress)
                        .font(.system(size: side * 0.06))
                        .foregroundStyle(PipColours.squareMuted)
                        .lineLimit(1)
                }
            }
        }
        // Clear of ✕ and ⤢ at the top corners: a band at the top, and
        // margins at the sides (the board's 30 pt and 44 pt at 300 pt).
        .padding(EdgeInsets(top: side * 0.1, leading: side * 0.12, bottom: side * 0.04, trailing: side * 0.12))
        .frame(width: side, height: side)
        .background(PipColours.square)
        .foregroundStyle(PipColours.squareInk)
    }

    private var name: some View {
        Text(sale.name)
            .font(.system(size: side * 0.057, weight: .bold))
            .lineLimit(1)
            .truncationMode(.tail)
    }

    private func amount(size: CGFloat) -> some View {
        Text(sale.amount)
            .font(Manrope.font(size: size))
            .monospacedDigit()
            .lineLimit(1)
            .minimumScaleFactor(0.5)
    }

    private var amountLine: some View {
        HStack(alignment: .firstTextBaseline, spacing: side * 0.015) {
            amount(size: side * 0.08)
            if let left = sale.timeLeft(now: now) {
                Text("· \(left)")
                    .font(.system(size: side * 0.043))
                    .foregroundStyle(PipColours.squareMuted)
                    .monospacedDigit()
            }
        }
        .lineLimit(1)
    }
}
