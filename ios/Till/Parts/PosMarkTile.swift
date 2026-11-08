import SwiftUI
import TillCore

/// A point of sale's mark on a tile, leading its row (`PosChoice`): Square's
/// square in a square, drawn in the theme's ink, and Clover's four leaves in
/// its green. Simple stand-ins for each brand's art; decoration only.
struct PosMarkTile: View {
    let mark: PosMark
    var inInk = true
    var size: CGFloat = 36

    private static let cloverGreen = Color(hex: 0x1DA544)

    var body: some View {
        RoundedRectangle(cornerRadius: size * 0.25)
            .fill(Palette.ground)
            .overlay(RoundedRectangle(cornerRadius: size * 0.25).stroke(Palette.border))
            .frame(width: size, height: size)
            .overlay { art.frame(width: size * 0.56, height: size * 0.56) }
            .accessibilityHidden(true)
    }

    @ViewBuilder
    private var art: some View {
        switch mark {
        case .square:
            let ink = inInk ? Palette.ink : Color.black
            GeometryReader { box in
                let side = box.size.width
                ZStack {
                    RoundedRectangle(cornerRadius: side * 0.22)
                        .stroke(ink, lineWidth: side * 0.14)
                    RoundedRectangle(cornerRadius: side * 0.06)
                        .fill(ink)
                        .frame(width: side * 0.36, height: side * 0.36)
                }
            }
        case .clover:
            let green = inInk ? Palette.ink : Self.cloverGreen
            Grid(horizontalSpacing: 2, verticalSpacing: 2) {
                GridRow {
                    Circle().fill(green)
                    Circle().fill(green)
                }
                GridRow {
                    Circle().fill(green)
                    Circle().fill(green)
                }
            }
        }
    }
}
