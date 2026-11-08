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
    /// The lock screen in dark mode: Android's dark ground and its text.
    static let darkGround = Color(hex: 0x1C1B19)
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

/// The Monokulo mark, from the app's `Mark.swift` (Android's `Mark.kt`):
/// the orange lens of facets in a ring. Still: a Live Activity doesn't
/// animate it.
struct WidgetMark: View {
    var size: CGFloat
    var ring: Color = WidgetPalette.cream

    var body: some View {
        ZStack {
            ZStack {
                MarkShape(kind: .lens).fill(MarkData.shades[0])
                MarkShape(kind: .facets(shade: 0)).fill(MarkData.shades[0])
                MarkShape(kind: .facets(shade: 1)).fill(MarkData.shades[1])
                MarkShape(kind: .facets(shade: 2)).fill(MarkData.shades[2])
            }
            .clipShape(MarkShape(kind: .lens))
            MarkShape(kind: .border).stroke(ring, lineWidth: MarkData.borderStroke * size / 64)
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}

/// The mark's measures, in the SVG's 64-unit box before the lens's 1.42.
private enum MarkData {
    static let shades: [Color] = [Color(hex: 0xFF6600), Color(hex: 0xE85D00), Color(hex: 0xC24E00)]
    static let centre = CGPoint(x: 30, y: 28)
    static let lensRadius: CGFloat = 15.75
    static let borderRadius: CGFloat = 18
    static let borderStroke: CGFloat = 4.5

    /// Each facet and its shade, in the SVG's order: the core, the inner
    /// six, the middle twelve, the rim twelve.
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

    /// A path from SVG data of absolute `M`, `L` and `Z`: all the mark uses.
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
            if token == "Z" {
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

/// One layer of the mark, fitted to its frame as the app's `drawMark` fits it.
private struct MarkShape: Shape {
    enum Kind: Sendable {
        case lens
        case border
        case facets(shade: Int)
    }

    let kind: Kind

    func path(in rect: CGRect) -> Path {
        let unit = min(rect.width, rect.height) / 64
        let fit = CGAffineTransform(
            translationX: rect.minX + (rect.width - 64 * unit) / 2,
            y: rect.minY + (rect.height - 64 * unit) / 2
        )
        .scaledBy(x: unit, y: unit)
        .translatedBy(x: 32, y: 32)
        .scaledBy(x: 1.42, y: 1.42)
        .translatedBy(x: -MarkData.centre.x, y: -MarkData.centre.y)
        var path = Path()
        switch kind {
        case .lens:
            path.addEllipse(in: circle(MarkData.lensRadius))
        case .border:
            path.addEllipse(in: circle(MarkData.borderRadius))
        case .facets(let shade):
            for facet in MarkData.facets where facet.shade == shade {
                path.addPath(MarkData.path(facet.d))
            }
        }
        return path.applying(fit)
    }

    private func circle(_ radius: CGFloat) -> CGRect {
        CGRect(x: MarkData.centre.x - radius, y: MarkData.centre.y - radius, width: 2 * radius, height: 2 * radius)
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
