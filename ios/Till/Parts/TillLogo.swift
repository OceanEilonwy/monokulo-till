import SwiftUI

/// What the Till logo shows (the owner's TillWidget and TillRound, in
/// docs/designs/ipad-redesign/): idle, then a payment's steps.
enum TillLogoState: Equatable, Sendable {
    case idle
    case waiting
    case seen
    case confirming
    case paid

    /// Which of the three keys are lit: none waiting, one seen, two
    /// confirming, all three (green) paid.
    var step: Int {
        switch self {
        case .idle, .waiting: 0
        case .seen: 1
        case .confirming: 2
        case .paid: 3
        }
    }

    /// How much of the bar (the round logo's ring) is filled, 0 to 1.
    var progress: CGFloat {
        self == .paid ? 1 : CGFloat(step) / 3
    }

    /// What a screen reader says for it.
    var label: String {
        switch self {
        case .idle: "Till, idle"
        case .waiting: "Till, waiting for a payment"
        case .seen: "Till, payment seen"
        case .confirming: "Till, confirming"
        case .paid: "Till, paid"
        }
    }

    /// The logo for a sale at the engine's `step`: 1 waiting, 2 seen, 3
    /// confirming, 4 paid.
    static func of(step: Int) -> TillLogoState {
        switch step {
        case ...1: .waiting
        case 2: .seen
        case 3: .confirming
        default: .paid
        }
    }
}

/// The logo's colours in `state`, as the boards' `renderVals` choose them
/// (the "keys" variant).
struct TillLogoColours: Equatable {
    static let orange: UInt32 = 0xFF6600
    static let green: UInt32 = 0x2EA052

    var body: UInt32
    var dim: UInt32
    var mid: UInt32
    var warm: UInt32
    var deep: UInt32
    var keys: [UInt32]
    var barFill: UInt32
    /// The widget's bezel round its screen, and the round logo's.
    var widgetOutline: UInt32
    var roundOutline: UInt32

    init(state: TillLogoState, dark: Bool) {
        let paid = state == .paid
        let dim: UInt32 = dark ? 0x66615A : 0x4A4740
        self.dim = dim
        body = dark ? 0x3A3733 : 0x1A1917
        widgetOutline = dark ? 0x1E1E1E : 0xF4F1EA
        roundOutline = dark ? 0x1A1917 : 0x4A4740
        switch state {
        case .paid:
            (mid, warm, deep) = (0x2EA052, 0x238A43, 0x1A7F37)
        case .idle:
            (mid, warm, deep) = (0x7F7F7F, 0x747474, 0x616161)
        default:
            (mid, warm, deep) = (0xFF6600, 0xE85D00, 0xC24E00)
        }
        let step = state.step
        keys = [
            paid ? Self.green : step >= 1 ? Self.orange : dim,
            paid ? Self.green : step >= 2 ? Self.orange : dim,
            paid ? Self.green : dim,
        ]
        barFill = paid ? Self.green : Self.orange
    }
}

/// The logo's drawing, in the boards' SVG units.
enum TillLogoGeometry {
    /// The widget's viewBox, and the round logo's.
    static let widgetBox = CGRect(x: 8, y: 8, width: 104, height: 104)
    static let roundBox = CGRect(x: 10, y: 10, width: 100, height: 100)

    static let body = CGRect(x: 12, y: 12, width: 96, height: 96)
    static let bodyRadius: CGFloat = 18
    static let keys = [
        CGRect(x: 80, y: 28, width: 16, height: 11),
        CGRect(x: 80, y: 45.5, width: 16, height: 11),
        CGRect(x: 80, y: 63, width: 16, height: 11),
    ]
    static let keyRadius: CGFloat = 3
    static let bar = CGRect(x: 24, y: 88, width: 72, height: 7)
    static let screen = CGRect(x: 26.5, y: 29.5, width: 43, height: 43)
    static let screenRadius: CGFloat = 6.5
    /// The lens's eight facets, in the warm shade over the screen.
    static let facets: [[CGPoint]] = [
        [point(43, 46), point(53, 46), point(48, 39.5)],
        [point(53, 46), point(53, 56), point(59.5, 51)],
        [point(53, 56), point(43, 56), point(48, 62.5)],
        [point(43, 56), point(43, 46), point(36.5, 51)],
        [point(48, 39.5), point(26.5, 29.5), point(69.5, 29.5)],
        [point(59.5, 51), point(69.5, 29.5), point(69.5, 72.5)],
        [point(48, 62.5), point(69.5, 72.5), point(26.5, 72.5)],
        [point(36.5, 51), point(26.5, 72.5), point(26.5, 29.5)],
    ]
    /// The lens's core, in the deep shade.
    static let core = CGRect(x: 43, y: 46, width: 10, height: 10)
    /// The paid tick.
    static let tick = [CGPoint(x: 37.5, y: 51.5), CGPoint(x: 44.5, y: 58.5), CGPoint(x: 58.5, y: 43.5)]
    /// The bezel round the screen, stroked 5 wide.
    static let outline = CGRect(x: 24, y: 27, width: 48, height: 48)
    static let outlineRadius: CGFloat = 9
    static let stroke: CGFloat = 5
    /// The round logo's disc, and how its terminal sits in it: the SVG's
    /// `translate(60 60) scale(1.22) translate(-48 -51)`.
    static let disc = CGRect(x: 10, y: 10, width: 100, height: 100)
    static let roundScale: CGFloat = 1.22
    static var roundInner: CGAffineTransform {
        CGAffineTransform(translationX: -48, y: -51)
            .concatenating(CGAffineTransform(scaleX: roundScale, y: roundScale))
            .concatenating(CGAffineTransform(translationX: 60, y: 60))
    }

    private static func point(_ x: CGFloat, _ y: CGFloat) -> CGPoint {
        CGPoint(x: x, y: y)
    }

    /// The bezel as one stroke that starts at its top middle and runs
    /// clockwise, which the round logo's progress trims.
    static func outlineFromTop() -> Path {
        let r = outline
        let radius = outlineRadius
        var path = Path()
        path.move(to: CGPoint(x: r.midX, y: r.minY))
        path.addLine(to: CGPoint(x: r.maxX - radius, y: r.minY))
        path.addArc(tangent1End: CGPoint(x: r.maxX, y: r.minY), tangent2End: CGPoint(x: r.maxX, y: r.minY + radius), radius: radius)
        path.addLine(to: CGPoint(x: r.maxX, y: r.maxY - radius))
        path.addArc(tangent1End: CGPoint(x: r.maxX, y: r.maxY), tangent2End: CGPoint(x: r.maxX - radius, y: r.maxY), radius: radius)
        path.addLine(to: CGPoint(x: r.minX + radius, y: r.maxY))
        path.addArc(tangent1End: CGPoint(x: r.minX, y: r.maxY), tangent2End: CGPoint(x: r.minX, y: r.maxY - radius), radius: radius)
        path.addLine(to: CGPoint(x: r.minX, y: r.minY + radius))
        path.addArc(tangent1End: CGPoint(x: r.minX, y: r.minY), tangent2End: CGPoint(x: r.minX + radius, y: r.minY), radius: radius)
        path.closeSubpath()
        return path
    }
}

/// One part of the logo, drawn in its viewBox's units and fitted to the
/// view: the box scaled to the view's width, and set inside the round
/// logo's disc when `round`.
struct TillLogoPart: Shape {
    enum Kind: Sendable {
        case rounded(CGRect, CGFloat)
        case polygons([[CGPoint]])
        case polyline([CGPoint])
        case disc(CGRect)
        case outlineFromTop
        /// The bar's fill: the bar, cut off `progress` of the way along.
        case barFill(CGFloat)
    }

    var kind: Kind
    var box: CGRect
    /// Drawn inside the round logo's disc, by its transform.
    var round = false

    var animatableData: CGFloat {
        get {
            if case .barFill(let progress) = kind { return progress }
            return 0
        }
        set {
            if case .barFill = kind { kind = .barFill(newValue) }
        }
    }

    func path(in rect: CGRect) -> Path {
        let scale = rect.width / box.width
        let fit = (round ? TillLogoGeometry.roundInner : .identity)
            .concatenating(CGAffineTransform(translationX: -box.minX, y: -box.minY))
            .concatenating(CGAffineTransform(scaleX: scale, y: scale))
            .concatenating(CGAffineTransform(translationX: rect.minX, y: rect.minY))
        return raw.applying(fit)
    }

    private var raw: Path {
        switch kind {
        case .rounded(let rect, let radius):
            return Path(roundedRect: rect, cornerRadius: radius, style: .circular)
        case .polygons(let polygons):
            var path = Path()
            for points in polygons {
                path.addLines(points)
                path.closeSubpath()
            }
            return path
        case .polyline(let points):
            var path = Path()
            path.addLines(points)
            return path
        case .disc(let rect):
            return Path(ellipseIn: rect)
        case .outlineFromTop:
            return TillLogoGeometry.outlineFromTop()
        case .barFill(let progress):
            let bar = TillLogoGeometry.bar
            let capsule = Path(roundedRect: bar, cornerRadius: bar.height / 2, style: .circular)
            let filled = Path(CGRect(x: bar.minX, y: bar.minY, width: bar.width * max(0, min(1, progress)), height: bar.height))
            return capsule.intersection(filled)
        }
    }
}

/// The Till logo, the terminal (TillWidget): a dark body with three keys
/// that light as a payment comes in, the Monokulo lens on its screen, and
/// a bar that fills, all green with a tick once paid. Waiting, the first
/// key blinks, unless Reduce Motion is on. A change of state animates (the
/// Paid moment): the bar fills, the colours turn and the tick draws on;
/// under Reduce Motion, a short crossfade.
struct TillWidget: View {
    var state: TillLogoState
    var size: CGFloat = 128
    /// Decoration: hidden from VoiceOver, as when a word beside it says the state.
    var decorative = false

    @Environment(\.colorScheme) private var scheme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        let colours = TillLogoColours(state: state, dark: scheme == .dark)
        let box = TillLogoGeometry.widgetBox
        let scale = size / box.width
        ZStack {
            TillLogoPart(kind: .rounded(TillLogoGeometry.body, TillLogoGeometry.bodyRadius), box: box)
                .fill(Color(hex: colours.body))
            ForEach(0..<3, id: \.self) { index in
                TillLogoPart(kind: .rounded(TillLogoGeometry.keys[index], TillLogoGeometry.keyRadius), box: box)
                    .fill(Color(hex: colours.keys[index]))
            }
            if state == .waiting && !reduceMotion {
                BlinkingKey(box: box)
            }
            TillLogoPart(kind: .rounded(TillLogoGeometry.bar, TillLogoGeometry.bar.height / 2), box: box)
                .fill(Color(hex: colours.dim))
            TillLogoPart(kind: .barFill(state.progress), box: box)
                .fill(Color(hex: colours.barFill))
            TillScreenFace(colours: colours, paid: state == .paid, box: box, round: false, scale: scale, reduceMotion: reduceMotion)
            TillLogoPart(kind: .rounded(TillLogoGeometry.outline, TillLogoGeometry.outlineRadius), box: box)
                .stroke(Color(hex: colours.widgetOutline), lineWidth: TillLogoGeometry.stroke * scale)
        }
        .frame(width: size, height: size)
        .animation(reduceMotion ? .easeInOut(duration: 0.2) : .snappy(duration: 0.35), value: state)
        .modifier(LogoLabel(label: decorative ? nil : state.label))
    }
}

/// The Till logo in a disc (TillRound): the terminal's screen alone, its
/// bezel the progress ring, for small and round places.
struct TillRound: View {
    var state: TillLogoState
    var size: CGFloat = 64
    var decorative = false

    @Environment(\.colorScheme) private var scheme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        let colours = TillLogoColours(state: state, dark: scheme == .dark)
        let box = TillLogoGeometry.roundBox
        let scale = size / box.width
        let stroke = TillLogoGeometry.stroke * TillLogoGeometry.roundScale * scale
        ZStack {
            TillLogoPart(kind: .disc(TillLogoGeometry.disc), box: box)
                .fill(Color(hex: colours.body))
            TillScreenFace(colours: colours, paid: state == .paid, box: box, round: true, scale: scale * TillLogoGeometry.roundScale, reduceMotion: reduceMotion)
            TillLogoPart(kind: .outlineFromTop, box: box, round: round)
                .stroke(Color(hex: colours.roundOutline), lineWidth: stroke)
            TillLogoPart(kind: .outlineFromTop, box: box, round: round)
                .trim(from: 0, to: state.progress)
                .stroke(Color(hex: colours.barFill), style: StrokeStyle(lineWidth: stroke, lineCap: .round))
        }
        .frame(width: size, height: size)
        .animation(reduceMotion ? .easeInOut(duration: 0.2) : .snappy(duration: 0.35), value: state)
        .modifier(LogoLabel(label: decorative ? nil : state.label))
    }
}

/// The terminal's screen: the lens in its three shades, clipped to the
/// screen's rounded square, and the tick once paid.
private struct TillScreenFace: View {
    let colours: TillLogoColours
    let paid: Bool
    let box: CGRect
    let round: Bool
    let scale: CGFloat
    let reduceMotion: Bool

    var body: some View {
        ZStack {
            TillLogoPart(kind: .rounded(TillLogoGeometry.screen, TillLogoGeometry.screenRadius), box: box, round: round)
                .fill(Color(hex: colours.mid))
            TillLogoPart(kind: .polygons(TillLogoGeometry.facets), box: box, round: round)
                .fill(Color(hex: colours.warm))
            TillLogoPart(kind: .rounded(TillLogoGeometry.core, 0), box: box, round: round)
                .fill(Color(hex: colours.deep))
            TillLogoPart(kind: .polyline(TillLogoGeometry.tick), box: box, round: round)
                // Drawn on, or under Reduce Motion faded in.
                .trim(from: 0, to: paid || reduceMotion ? 1 : 0)
                .stroke(Color.white, style: StrokeStyle(lineWidth: TillLogoGeometry.stroke * scale, lineCap: .round, lineJoin: .round))
                .opacity(paid ? 1 : 0)
        }
        .clipShape(TillLogoPart(kind: .rounded(TillLogoGeometry.screen, TillLogoGeometry.screenRadius), box: box, round: round))
    }
}

/// Waiting: the first key blinks orange, 1.2 s a blink.
private struct BlinkingKey: View {
    let box: CGRect
    @State private var start = Date()

    var body: some View {
        TimelineView(.animation) { timeline in
            let t = timeline.date.timeIntervalSince(start)
            TillLogoPart(kind: .rounded(TillLogoGeometry.keys[0], TillLogoGeometry.keyRadius), box: box)
                .fill(Color(hex: TillLogoColours.orange))
                .opacity(BlinkTiming.opacity(t))
        }
    }
}

/// The waiting key's blink as a function of time: up and down, linear.
enum BlinkTiming {
    static let period = 1.2

    static func opacity(_ seconds: Double) -> Double {
        let p = seconds.truncatingRemainder(dividingBy: period) / period
        return 1 - abs(2 * (p < 0 ? p + 1 : p) - 1)
    }
}

/// The logo as one image to VoiceOver with `label`, or hidden as decoration.
private struct LogoLabel: ViewModifier {
    let label: String?

    @ViewBuilder
    func body(content: Content) -> some View {
        if let label {
            content
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(label)
                .accessibilityAddTraits(.isImage)
        } else {
            content.accessibilityHidden(true)
        }
    }
}
