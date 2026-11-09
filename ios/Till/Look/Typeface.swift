import CoreText
import SwiftUI

/// Manrope (SIL OFL 1.1, `Resources/manrope-OFL.txt`), bundled as its
/// variable font: 800 for amounts and display headings (docs/ios-plan.md
/// §4). Everything else is SF, the system's own fonts, as SwiftUI's text
/// styles (`.body`, `.headline`, …) give them.
enum Manrope {
    /// The variable font's weight axis, `'wght'`.
    private static let weightAxis = 0x7767_6874

    /// The bundled font file.
    static var url: URL? { Bundle.main.url(forResource: "Manrope", withExtension: "ttf") }

    /// The font's description, read once from the bundled file.
    nonisolated(unsafe) private static let descriptor: CTFontDescriptor? = {
        guard let url, let data = try? Data(contentsOf: url) else { return nil }
        return CTFontManagerCreateFontDescriptorFromData(data as CFData)
    }()

    /// Registers the font with the process at launch, so `Font.custom("Manrope", …)`
    /// finds it too; ``font(size:weight:)`` reads the file itself and doesn't need this.
    static func register() {
        guard let url else { return }
        _ = CTFontManagerRegisterFontsForURL(url as CFURL, .process, nil)
    }

    /// Whether the bundled font was found and read.
    static var isAvailable: Bool { descriptor != nil }

    /// Manrope at [size] points and [weight] on its axis (200–800); SF at
    /// the nearest weight if the font is missing, so text still shows.
    static func font(size: CGFloat, weight: CGFloat = 800) -> Font {
        guard let descriptor else { return .system(size: size, weight: .heavy) }
        let varied = CTFontDescriptorCreateCopyWithVariation(descriptor, NSNumber(value: weightAxis) as CFNumber, weight)
        return Font(CTFontCreateWithFontDescriptor(varied, size, nil))
    }
}

/// Display type: Manrope 800, tabular figures so amounts hold still, at a
/// size that grows with Dynamic Type
/// alongside [relativeTo], tracked in as the design is.
struct DisplayType: ViewModifier {
    @ScaledMetric private var size: CGFloat
    private let tracking: CGFloat

    init(size: CGFloat, tracking: CGFloat, relativeTo style: Font.TextStyle) {
        _size = ScaledMetric(wrappedValue: size, relativeTo: style)
        self.tracking = tracking
    }

    func body(content: Content) -> some View {
        content.font(Manrope.font(size: size)).tracking(tracking).monospacedDigit()
    }
}

extension View {
    /// Manrope 800 for an amount or a display heading: [size] points at the
    /// default text size, tracked in by [tracking] points (Android's `display`).
    func display(_ size: CGFloat, tracking: CGFloat = -0.4, relativeTo style: Font.TextStyle = .largeTitle) -> some View {
        modifier(DisplayType(size: size, tracking: tracking, relativeTo: style))
    }
}
