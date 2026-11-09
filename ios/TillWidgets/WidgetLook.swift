import CoreImage
import CoreImage.CIFilterBuiltins
import SwiftUI

/// The few colours the Live Activity uses (docs/ios-plan.md §4; the boards
/// Main, PhoneOrder, PhoneOrderQR and PhonePaid), copied from the app's
/// `Palette`: the extension can't import the app.
enum WidgetPalette {
    /// The island's text.
    static let cream = Color(hex: 0xF4F1EA)
    /// Second lines on black.
    static let muted = Color(hex: 0xB8B2A6)
    /// "Tap to show code", on black.
    static let link = Color(hex: 0xFF8A3D)
    /// Paid and watching, on black.
    static let green = Color(hex: 0x4CC27A)
    /// An expired code's clock, on black.
    static let expired = Color(hex: 0xE8C267)

    /// The lock screen in light mode: the app's ground and its text.
    static let ground = Color(hex: 0xF4F1EA)
    static let ink = Color(hex: 0x1A1917)
    static let secondary = Color(hex: 0x5A564C)
    static let paid = Color(hex: 0x1A7F37)
    static let expiredInk = Color(hex: 0x7D5E00)
    static let linkInk = Color(hex: 0xB34700)
    /// The lock screen in dark mode: the app's black ground and its text.
    static let darkGround = Color(hex: 0x000000)
    static let darkInk = Color(hex: 0xECE8DF)
    static let darkSecondary = Color(hex: 0xA8A29A)
}

extension Color {
    /// An opaque colour from `0xRRGGBB`.
    init(hex: UInt32) {
        self.init(
            .sRGB,
            red: Double((hex >> 16) & 0xFF) / 255,
            green: Double((hex >> 8) & 0xFF) / 255,
            blue: Double(hex & 0xFF) / 255,
            opacity: 1
        )
    }
}

/// Till's status mark: the app icon's terminal as Monokulo's "Keys count"
/// canvas draws it (TillWidget and TillRound), in two forms, square for the
/// expanded island and the lock screen, round for the compact and minimal
/// island's circles. Still: a Live Activity doesn't animate it, so the
/// canvas's flashing key and running segment are never drawn here, and the
/// colours and fills say the state, as they do under reduced motion.
enum TillStatus: Sendable {
    case idle, scanning, waiting, seen, confirming, paid

    /// The Live Activity's phase on the canvas's states: there is no
    /// "scanning" (the code waits for money, not for a scan the app can
    /// see), and an expired code shows its clock instead (never here).
    init(_ phase: TillActivityAttributes.ContentState.Phase) {
        switch phase {
        case .idle, .expired: self = .idle
        case .waiting: self = .waiting
        case .seen: self = .seen
        case .confirming: self = .confirming
        case .paid: self = .paid
        }
    }

    /// How many of the three steps are done: a third at seen, two at confirming, all once paid.
    var step: Int {
        switch self {
        case .idle, .scanning, .waiting: 0
        case .seen: 1
        case .confirming: 2
        case .paid: 3
        }
    }

    var isPaid: Bool { self == .paid }
    var progress: Double { Double(step) / 3 }

    var label: String {
        switch self {
        case .idle: "Till, idle"
        case .scanning: "Till, scanning"
        case .waiting: "Till, waiting for a payment"
        case .seen: "Till, payment seen"
        case .confirming: "Till, confirming"
        case .paid: "Till, paid"
        }
    }
}

/// The terminal's colours (the canvas's `renderVals`), in light and dark.
private struct TerminalInks {
    static let orange = Color(hex: 0xFF6600)
    static let green = Color(hex: 0x2EA052)

    let dark: Bool
    let status: TillStatus

    /// The unlit keys and the unfilled border and bar.
    var dim: Color { dark ? Color(hex: 0x66615A) : Color(hex: 0x4A4740) }
    var body: Color { dark ? Color(hex: 0x3A3733) : Color(hex: 0x1A1917) }
    /// The square's screen outline: the ground it sits on.
    var outline: Color { dark ? Color(hex: 0x1E1E1E) : Color(hex: 0xF4F1EA) }
    /// The round form's border track: dim on ink, ink on charcoal.
    var track: Color { dark ? Color(hex: 0x1A1917) : Color(hex: 0x4A4740) }

    /// The screen's three shades: the whole, the eight triangles, the centre square.
    var mid: Color { status.isPaid ? Color(hex: 0x2EA052) : status == .idle ? Color(hex: 0x7F7F7F) : Color(hex: 0xFF6600) }
    var warm: Color { status.isPaid ? Color(hex: 0x238A43) : status == .idle ? Color(hex: 0x747474) : Color(hex: 0xE85D00) }
    var deep: Color { status.isPaid ? Color(hex: 0x1A7F37) : status == .idle ? Color(hex: 0x616161) : Color(hex: 0xC24E00) }

    /// The three keys, top down: each joins at its step, all green once paid; the bottom one waits for paid.
    var keys: [Color] {
        if status.isPaid { return [Self.green, Self.green, Self.green] }
        return [status.step >= 1 ? Self.orange : dim, status.step >= 2 ? Self.orange : dim, dim]
    }

    /// The fill's colour: orange, green once paid.
    var fill: Color { status.isPaid ? Self.green : Self.orange }
}

/// The terminal's measures, on the canvas's 120 grid.
private enum TerminalData {
    static let screen = CGRect(x: 26.5, y: 29.5, width: 43, height: 43)
    static let screenRadius: CGFloat = 6.5
    static let body = CGRect(x: 12, y: 12, width: 96, height: 96)
    static let bodyRadius: CGFloat = 18
    static let keys = [
        CGRect(x: 80, y: 28, width: 16, height: 11),
        CGRect(x: 80, y: 45.5, width: 16, height: 11),
        CGRect(x: 80, y: 63, width: 16, height: 11),
    ]
    static let keyRadius: CGFloat = 3
    static let bar = CGRect(x: 24, y: 88, width: 72, height: 7)
    static let barRadius: CGFloat = 3.5
    /// The screen's border, drawn on the centre line of its stroke.
    static let border = CGRect(x: 24, y: 27, width: 48, height: 48)
    static let borderRadius: CGFloat = 9
    static let borderStroke: CGFloat = 5

    /// The eight triangles round the centre square.
    static let triangles: [[CGPoint]] = [
        [CGPoint(x: 43, y: 46), CGPoint(x: 53, y: 46), CGPoint(x: 48, y: 39.5)],
        [CGPoint(x: 53, y: 46), CGPoint(x: 53, y: 56), CGPoint(x: 59.5, y: 51)],
        [CGPoint(x: 53, y: 56), CGPoint(x: 43, y: 56), CGPoint(x: 48, y: 62.5)],
        [CGPoint(x: 43, y: 56), CGPoint(x: 43, y: 46), CGPoint(x: 36.5, y: 51)],
        [CGPoint(x: 48, y: 39.5), CGPoint(x: 26.5, y: 29.5), CGPoint(x: 69.5, y: 29.5)],
        [CGPoint(x: 59.5, y: 51), CGPoint(x: 69.5, y: 29.5), CGPoint(x: 69.5, y: 72.5)],
        [CGPoint(x: 48, y: 62.5), CGPoint(x: 69.5, y: 72.5), CGPoint(x: 26.5, y: 72.5)],
        [CGPoint(x: 36.5, y: 51), CGPoint(x: 26.5, y: 72.5), CGPoint(x: 26.5, y: 29.5)],
    ]
    static let centre = CGRect(x: 43, y: 46, width: 10, height: 10)

    /// The paid screen's tick.
    static let tick = [CGPoint(x: 37.5, y: 51.5), CGPoint(x: 44.5, y: 58.5), CGPoint(x: 58.5, y: 43.5)]
    static let tickStroke: CGFloat = 5
}

/// How the 120 grid is fitted to a frame: the grid's [box] is the frame,
/// with the grid point [pivot] at its centre and everything [scale]d
/// about it. The square form's box is the grid's `8 8 104 104`, about its
/// own centre; the round form's `10 10 100 100`, about the screen's
/// centre (48, 51), scaled 1.22.
private struct Fit: Sendable {
    let box: CGRect
    let pivot: CGPoint
    let scale: CGFloat

    static let square = Fit(box: CGRect(x: 8, y: 8, width: 104, height: 104), pivot: CGPoint(x: 60, y: 60), scale: 1)
    static let round = Fit(box: CGRect(x: 10, y: 10, width: 100, height: 100), pivot: CGPoint(x: 48, y: 51), scale: 1.22)

    /// One grid unit in points in a frame [size] across, for stroke widths.
    func point(_ size: CGFloat) -> CGFloat { size / box.width * scale }
}

/// A path drawn on the 120 grid, fitted to its frame.
private struct GridShape: Shape {
    let fit: Fit
    let build: @Sendable () -> Path

    func path(in rect: CGRect) -> Path {
        let unit = min(rect.width, rect.height) / fit.box.width
        let transform = CGAffineTransform(
            translationX: rect.minX + (rect.width - fit.box.width * unit) / 2,
            y: rect.minY + (rect.height - fit.box.height * unit) / 2
        )
        .scaledBy(x: unit, y: unit)
        .translatedBy(x: fit.box.width / 2, y: fit.box.height / 2)
        .scaledBy(x: fit.scale, y: fit.scale)
        .translatedBy(x: -fit.pivot.x, y: -fit.pivot.y)
        return build().applying(transform)
    }
}

private extension Path {
    /// The border of the screen from 12 o'clock, clockwise, as one closed path so a trim runs round it.
    static func border() -> Path {
        let r = TerminalData.border
        let c = TerminalData.borderRadius
        let k = c * 0.5523
        var path = Path()
        path.move(to: CGPoint(x: r.midX, y: r.minY))
        path.addLine(to: CGPoint(x: r.maxX - c, y: r.minY))
        path.addCurve(to: CGPoint(x: r.maxX, y: r.minY + c), control1: CGPoint(x: r.maxX - c + k, y: r.minY), control2: CGPoint(x: r.maxX, y: r.minY + c - k))
        path.addLine(to: CGPoint(x: r.maxX, y: r.maxY - c))
        path.addCurve(to: CGPoint(x: r.maxX - c, y: r.maxY), control1: CGPoint(x: r.maxX, y: r.maxY - c + k), control2: CGPoint(x: r.maxX - c + k, y: r.maxY))
        path.addLine(to: CGPoint(x: r.minX + c, y: r.maxY))
        path.addCurve(to: CGPoint(x: r.minX, y: r.maxY - c), control1: CGPoint(x: r.minX + c - k, y: r.maxY), control2: CGPoint(x: r.minX, y: r.maxY - c + k))
        path.addLine(to: CGPoint(x: r.minX, y: r.minY + c))
        path.addCurve(to: CGPoint(x: r.minX + c, y: r.minY), control1: CGPoint(x: r.minX, y: r.minY + c - k), control2: CGPoint(x: r.minX + c - k, y: r.minY))
        path.closeSubpath()
        return path
    }

    static func rounded(_ rect: CGRect, _ radius: CGFloat) -> Path {
        Path(roundedRect: rect, cornerRadius: radius, style: .circular)
    }

    static func triangles() -> Path {
        var path = Path()
        for points in TerminalData.triangles {
            path.move(to: points[0])
            path.addLine(to: points[1])
            path.addLine(to: points[2])
            path.closeSubpath()
        }
        return path
    }

    static func tick() -> Path {
        var path = Path()
        path.move(to: TerminalData.tick[0])
        path.addLine(to: TerminalData.tick[1])
        path.addLine(to: TerminalData.tick[2])
        return path
    }
}

/// The screen in its three shades, with the paid tick, clipped to its rounded square.
private struct TerminalScreen: View {
    let fit: Fit
    let size: CGFloat
    let inks: TerminalInks

    var body: some View {
        ZStack {
            GridShape(fit: fit) { .rounded(TerminalData.screen, TerminalData.screenRadius) }.fill(inks.mid)
            GridShape(fit: fit) { .triangles() }.fill(inks.warm)
            GridShape(fit: fit) { Path(TerminalData.centre) }.fill(inks.deep)
            if inks.status.isPaid {
                GridShape(fit: fit) { .tick() }
                    .stroke(Color.white, style: StrokeStyle(lineWidth: TerminalData.tickStroke * fit.point(size), lineCap: .round, lineJoin: .round))
            }
        }
        .clipShape(GridShape(fit: fit) { .rounded(TerminalData.screen, TerminalData.screenRadius) })
    }
}

/// The square form (TillWidget, `keys`): the terminal with its three keys
/// counting the steps and its bottom bar as the progress. For the expanded
/// island and the lock screen. [dark] is the charcoal body.
struct WidgetTerminal: View {
    var status: TillStatus
    var size: CGFloat
    var dark: Bool = false

    var body: some View {
        let inks = TerminalInks(dark: dark, status: status)
        let fit = Fit.square
        let bar = TerminalData.bar
        let filled = CGRect(x: bar.minX, y: bar.minY, width: bar.width * status.progress, height: bar.height)
        ZStack {
            GridShape(fit: fit) { .rounded(TerminalData.body, TerminalData.bodyRadius) }.fill(inks.body)
            ForEach(0..<3, id: \.self) { i in
                GridShape(fit: fit) { .rounded(TerminalData.keys[i], TerminalData.keyRadius) }.fill(inks.keys[i])
            }
            GridShape(fit: fit) { .rounded(bar, TerminalData.barRadius) }.fill(inks.dim)
            if status.step > 0 {
                GridShape(fit: fit) { .rounded(bar, TerminalData.barRadius) }
                    .fill(inks.fill)
                    .mask(GridShape(fit: fit) { Path(filled) })
            }
            TerminalScreen(fit: fit, size: size, inks: inks)
            GridShape(fit: fit) { .rounded(TerminalData.border, TerminalData.borderRadius) }
                .stroke(inks.outline, lineWidth: TerminalData.borderStroke * fit.point(size))
        }
        .frame(width: size, height: size)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(status.label)
        .accessibilityAddTraits(.isImage)
    }
}

/// The round form (TillRound), for circular containers: the screen alone
/// on a disc, scaled 1.22, with no keys and no outer ring; the progress
/// runs clockwise round the screen's own border from 12 o'clock, a third a
/// step, whole and green once paid. Its track is dim on the ink disc and
/// ink on the dark one's charcoal.
struct WidgetRound: View {
    var status: TillStatus
    var size: CGFloat
    var dark: Bool = false

    var body: some View {
        let inks = TerminalInks(dark: dark, status: status)
        let fit = Fit.round
        let width = TerminalData.borderStroke * fit.point(size)
        ZStack {
            Circle().fill(inks.body)
            TerminalScreen(fit: fit, size: size, inks: inks)
            GridShape(fit: fit) { .border() }
                .stroke(inks.track, lineWidth: width)
            if status.step > 0 {
                GridShape(fit: fit) { .border() }
                    .trim(from: 0, to: status.progress)
                    .stroke(inks.fill, style: StrokeStyle(lineWidth: width, lineCap: status.isPaid ? .butt : .round))
            }
        }
        .frame(width: size, height: size)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(status.label)
        .accessibilityAddTraits(.isImage)
    }
}

/// The payment code on its white card, small, as PhoneOrderQR draws it in
/// the island: Core Image's `CIQRCodeGenerator` at correction level M, ink
/// on white, scaled up with no smoothing.
struct WidgetQR: View {
    let text: String
    var size: CGFloat

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: size * 0.2, style: .continuous)
                .fill(Color.white)
            if let image = WidgetQR.image(text) {
                Image(decorative: image, scale: 1)
                    .resizable()
                    .interpolation(.none)
                    .antialiased(false)
                    .aspectRatio(1, contentMode: .fit)
                    .padding(size * 0.06)
            }
        }
        .frame(width: size, height: size)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Payment code")
        .accessibilityAddTraits(.isImage)
    }

    /// `text`'s code, one pixel a module, with the generator's own quiet zone.
    static func image(_ text: String) -> CGImage? {
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(text.utf8)
        filter.correctionLevel = "M"
        guard let output = filter.outputImage else { return nil }
        return CIContext().createCGImage(output, from: output.extent.integral)
    }
}
