import SwiftUI

/// The Monokulo mark's measures and paths, ported from Android's
/// `ui/Mark.kt` (the shipped source of truth, the web's SVG), in mark
/// units: the SVG's 64-unit box, before the lens's own 1.42 scale.
enum MarkGeometry {
    /// The lens's centre and radius: every facet is clipped to it.
    static let centre = CGPoint(x: 30, y: 28)
    static let lensRadius: CGFloat = 15.75

    /// The border round the lens: its radius and stroke.
    static let borderRadius: CGFloat = 18
    static let borderStroke: CGFloat = 4.5

    /// The lens's three shades, light to dark: #ff6600, #e85d00, #c24e00.
    static let shades: [Color] = [Color(hex: 0xFF6600), Color(hex: 0xE85D00), Color(hex: 0xC24E00)]

    /// The shade of the lens's base circle, under the facets: the middle twelve's.
    static let baseShade = 0

    /// The glints' and the core's colour.
    static let glint = Color(hex: 0xFFF3E8)

    /// Each facet and its shade (an index into ``shades``), in the SVG's
    /// order: the core, the inner six, the middle twelve, the rim twelve.
    static let facets: [(d: String, shade: Int)] = [
        ("M30,23L34.33,25.5L34.33,30.5L30,33L25.67,30.5L25.67,25.5Z", 2),
        ("M30,23L34.33,25.5L35.5,18.47Z", 1),
        ("M34.33,25.5L34.33,30.5L41,28Z", 1),
        ("M34.33,30.5L30,33L35.5,37.53Z", 1),
        ("M30,33L25.67,30.5L24.5,37.53Z", 1),
        ("M25.67,30.5L25.67,25.5L19,28Z", 1),
        ("M25.67,25.5L30,23L24.5,18.47Z", 1),
        ("M30,23L24.5,18.47L30,4Z", 0),
        ("M30,23L30,4L35.5,18.47Z", 0),
        ("M34.33,25.5L35.5,18.47L50.78,16Z", 0),
        ("M34.33,25.5L50.78,16L41,28Z", 0),
        ("M34.33,30.5L41,28L50.78,40Z", 0),
        ("M34.33,30.5L50.78,40L35.5,37.53Z", 0),
        ("M30,33L35.5,37.53L30,52Z", 0),
        ("M30,33L30,52L24.5,37.53Z", 0),
        ("M25.67,30.5L24.5,37.53L9.22,40Z", 0),
        ("M25.67,30.5L9.22,40L19,28Z", 0),
        ("M25.67,25.5L19,28L9.22,16Z", 0),
        ("M25.67,25.5L9.22,16L24.5,18.47Z", 0),
        ("M35.5,18.47L30,4L42,7.22Z", 1),
        ("M35.5,18.47L42,7.22L50.78,16Z", 1),
        ("M41,28L50.78,16L54,28Z", 1),
        ("M41,28L54,28L50.78,40Z", 1),
        ("M35.5,37.53L50.78,40L42,48.78Z", 1),
        ("M35.5,37.53L42,48.78L30,52Z", 1),
        ("M24.5,37.53L30,52L18,48.78Z", 1),
        ("M24.5,37.53L18,48.78L9.22,40Z", 1),
        ("M19,28L9.22,40L6,28Z", 1),
        ("M19,28L6,28L9.22,16Z", 1),
        ("M24.5,18.47L9.22,16L18,7.22Z", 1),
        ("M24.5,18.47L18,7.22L30,4Z", 1),
    ]

    /// The core hexagon, which pulses on the loading mark.
    static let core = "M30,23L34.33,25.5L34.33,30.5L30,33L25.67,30.5L25.67,25.5Z"

    /// The loading mark's glints, each with its delay in ms into one turn of
    /// ``LoadingMarkTiming/glintMs``: Mark.kt's seventeen spokes.
    static let spokes: [(d: String, delayMs: Double)] = [
        ("M35.5,18.47L30,4L42,7.22Z", 0),
        ("M30,23L30,4L35.5,18.47Z", 92),
        ("M30,23L34.33,25.5L35.5,18.47Z", 181),
        ("M35.5,37.53L42,48.78L30,52Z", 701),
        ("M30,33L35.5,37.53L30,52Z", 790),
        ("M34.33,30.5L30,33L35.5,37.53Z", 882),
        ("M24.5,18.47L9.22,16L18,7.22Z", 1399),
        ("M25.67,25.5L9.22,16L24.5,18.47Z", 1491),
        ("M25.67,25.5L30,23L24.5,18.47Z", 1583),
        ("M41,28L50.78,16L54,28Z", 2100),
        ("M34.33,25.5L50.78,16L41,28Z", 2192),
        ("M34.33,25.5L34.33,30.5L41,28Z", 2281),
        ("M24.5,37.53L18,48.78L9.22,40Z", 2801),
        ("M25.67,30.5L24.5,37.53L9.22,40Z", 2890),
        ("M30,33L25.67,30.5L24.5,37.53Z", 2982),
        ("M35.5,37.53L50.78,40L42,48.78Z", 3499),
        ("M34.33,30.5L50.78,40L35.5,37.53Z", 3591),
    ]

    /// A path from SVG path data of absolute moves, lines and closes (`M`,
    /// `L`, `Z`, numbers split by commas or spaces): all the mark uses.
    static func path(_ d: String) -> Path {
        var tokens: [String] = []
        var number = ""
        for character in d {
            if character.isLetter || character == "," || character == " " {
                if !number.isEmpty {
                    tokens.append(number)
                    number = ""
                }
                if character.isLetter { tokens.append(String(character)) }
            } else {
                number.append(character)
            }
        }
        if !number.isEmpty { tokens.append(number) }

        var path = Path()
        var command = "M"
        var i = 0
        while i < tokens.count {
            let token = tokens[i]
            if token == "Z" || token == "z" {
                path.closeSubpath()
                i += 1
            } else if token == "M" || token == "L" {
                command = token
                i += 1
            } else {
                guard i + 1 < tokens.count, let x = Double(tokens[i]), let y = Double(tokens[i + 1]) else { break }
                let point = CGPoint(x: x, y: y)
                if command == "M" {
                    path.move(to: point)
                    command = "L"
                } else {
                    path.addLine(to: point)
                }
                i += 2
            }
        }
        return path
    }
}

/// The loading mark's two loops (`mk-glint` and `mk-core` in the web's CSS;
/// Mark.kt's `glintAlpha` and `coreAlpha`), as pure functions of time.
enum LoadingMarkTiming {
    /// One turn of the glints round the lens, linear and repeating.
    static let glintMs = 4200.0
    /// One pulse of the core.
    static let coreMs = 700.0

    /// A glint's opacity `p` of the way through its turn: in over 1%, held
    /// to 9%, gone by 12%.
    static func glintAlpha(_ p: Double) -> Double {
        if p < 0.01 { return 0.95 * p / 0.01 }
        if p < 0.09 { return 0.95 }
        if p < 0.12 { return 0.95 * (0.12 - p) / 0.03 }
        return 0
    }

    /// The core's opacity `p` of the way through its pulse.
    static func coreAlpha(_ p: Double) -> Double {
        if p < 0.39 { return 0 }
        if p < 0.48 { return 0.95 * (p - 0.39) / 0.09 }
        if p < 0.78 { return 0.95 }
        if p < 0.96 { return 0.95 * (0.96 - p) / 0.18 }
        return 0
    }

    /// How far through its turn, 0 to 1, a glint with [delayMs] is at [timeMs].
    static func glintProgress(timeMs: Double, delayMs: Double) -> Double {
        let t = (timeMs - delayMs).truncatingRemainder(dividingBy: glintMs)
        return (t < 0 ? t + glintMs : t) / glintMs
    }

    /// How far through its pulse, 0 to 1, the core is at [timeMs].
    static func coreProgress(timeMs: Double) -> Double {
        let t = timeMs.truncatingRemainder(dividingBy: coreMs)
        return (t < 0 ? t + coreMs : t) / coreMs
    }
}

/// The mark's paths, parsed once.
private enum MarkPaths {
    static let lens = Path(ellipseIn: CGRect(
        x: MarkGeometry.centre.x - MarkGeometry.lensRadius,
        y: MarkGeometry.centre.y - MarkGeometry.lensRadius,
        width: 2 * MarkGeometry.lensRadius,
        height: 2 * MarkGeometry.lensRadius
    ))
    static let border = Path(ellipseIn: CGRect(
        x: MarkGeometry.centre.x - MarkGeometry.borderRadius,
        y: MarkGeometry.centre.y - MarkGeometry.borderRadius,
        width: 2 * MarkGeometry.borderRadius,
        height: 2 * MarkGeometry.borderRadius
    ))
    static let facets: [(path: Path, shade: Int)] = MarkGeometry.facets.map { (path: MarkGeometry.path($0.d), shade: $0.shade) }
    static let core = MarkGeometry.path(MarkGeometry.core)
    static let spokes: [(path: Path, delayMs: Double)] = MarkGeometry.spokes.map { (path: MarkGeometry.path($0.d), delayMs: $0.delayMs) }
}

/// Draws the mark into a canvas: the 64-unit box fitted to its smaller
/// side, the lens scaled about its centre as the SVG does. With [timeMs],
/// the loading mark's glints and core at that moment.
func drawMark(_ context: inout GraphicsContext, size: CGSize, ring: Color, timeMs: Double?) {
    let unit = min(size.width, size.height) / 64
    context.translateBy(x: (size.width - 64 * unit) / 2, y: (size.height - 64 * unit) / 2)
    context.scaleBy(x: unit, y: unit)
    context.translateBy(x: 32, y: 32)
    context.scaleBy(x: 1.42, y: 1.42)
    context.translateBy(x: -MarkGeometry.centre.x, y: -MarkGeometry.centre.y)

    var lens = context
    lens.clip(to: MarkPaths.lens)
    lens.fill(MarkPaths.lens, with: .color(MarkGeometry.shades[MarkGeometry.baseShade]))
    for facet in MarkPaths.facets {
        lens.fill(facet.path, with: .color(MarkGeometry.shades[facet.shade]))
    }
    if let timeMs {
        let coreAlpha = LoadingMarkTiming.coreAlpha(LoadingMarkTiming.coreProgress(timeMs: timeMs))
        if coreAlpha > 0 {
            var core = lens
            core.opacity = coreAlpha
            core.fill(MarkPaths.core, with: .color(MarkGeometry.glint))
        }
        for spoke in MarkPaths.spokes {
            let alpha = LoadingMarkTiming.glintAlpha(LoadingMarkTiming.glintProgress(timeMs: timeMs, delayMs: spoke.delayMs))
            if alpha > 0 {
                var glint = lens
                glint.opacity = alpha
                glint.fill(spoke.path, with: .color(MarkGeometry.glint))
            }
        }
    }
    context.stroke(MarkPaths.border, with: .color(ring), lineWidth: MarkGeometry.borderStroke)
}

/// The Monokulo logo (the design's Logo part): an orange lens of facets in
/// a ring of [ring]'s colour. Decoration unless given a [label].
struct Logo: View {
    var size: CGFloat
    var ring: Color = Palette.ink
    var label: String? = nil

    var body: some View {
        Canvas { context, canvasSize in
            drawMark(&context, size: canvasSize, ring: ring, timeMs: nil)
        }
        .frame(width: size, height: size)
        .modifier(MarkLabel(label: label))
    }
}

/// The loading mark (the design's LoadingMark part): the logo with glints
/// running round it and its core pulsing, which says "watching" or
/// "waiting". Still when the person has asked for less motion.
struct LoadingMark: View {
    var size: CGFloat
    var label: String
    var ring: Color = Palette.ink

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var start = Date()

    var body: some View {
        Group {
            if reduceMotion {
                Canvas { context, canvasSize in
                    drawMark(&context, size: canvasSize, ring: ring, timeMs: nil)
                }
            } else {
                TimelineView(.animation) { timeline in
                    let timeMs = timeline.date.timeIntervalSince(start) * 1000
                    Canvas { context, canvasSize in
                        drawMark(&context, size: canvasSize, ring: ring, timeMs: timeMs)
                    }
                }
            }
        }
        .frame(width: size, height: size)
        .modifier(MarkLabel(label: label))
    }
}

/// The mark as one image to VoiceOver with [label], or hidden as decoration.
private struct MarkLabel: ViewModifier {
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
