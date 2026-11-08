import SwiftUI
import UIKit

/// The design's colours by role (docs/ios-plan.md §4). Light is the design;
/// dark is Android's dark theme (`ui/Theme.kt`'s `DarkTokens`): the same
/// roles on a warm dark ground, each text pair at least 4.5:1.
enum Palette {
    /// The page.
    static let ground = Color.adaptive(light: 0xF4F1EA, dark: 0x1C1B19)
    /// Text and icons.
    static let ink = Color.adaptive(light: 0x1A1917, dark: 0xECE8DF)
    /// Secondary text.
    static let secondary = Color.adaptive(light: 0x5A564C, dark: 0xA8A29A)
    /// Cards and grouped rows.
    static let card = Color.adaptive(light: 0xFFFFFF, dark: 0x2A2825)
    /// The one primary action, with ``onAccent`` on it: the same in both themes.
    static let accent = Color(hex: 0xFF6600)
    static let onAccent = Color(hex: 0x111111)
    /// Text links and small accents.
    static let link = Color.adaptive(light: 0xB34700, dark: 0xFF8A3D)
    /// Paid, as text and ticks.
    static let paid = Color.adaptive(light: 0x1A7F37, dark: 0x4CC27A)
    /// Disconnect; a reversed payment.
    static let danger = Color.adaptive(light: 0xB00020, dark: 0xFF6B6B)
    /// Lines between rows inside a card.
    static let hairline = Color.adaptive(light: 0xECE6DA, dark: 0x3A3733)
    /// A card's or a quiet button's edge.
    static let border = Color.adaptive(light: 0xDCD5C7, dark: 0x4A4743)
    /// An expired code's note: its ground and its text.
    static let expiredGround = Color.adaptive(light: 0xF1E3CF, dark: 0x3D3320)
    static let expiredInk = Color.adaptive(light: 0x7D5E00, dark: 0xE8C267)

    /// The QR code's modules and card, ink on white whatever the theme.
    static let codeInk = Color(hex: 0x1A1917)
    static let codeGround = Color.white
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

    /// A colour that follows the system's light or dark appearance.
    static func adaptive(light: UInt32, dark: UInt32) -> Color {
        Color(uiColor: UIColor { traits in
            UIColor(hex: traits.userInterfaceStyle == .dark ? dark : light)
        })
    }
}

extension UIColor {
    /// An opaque colour from `0xRRGGBB`.
    convenience init(hex: UInt32) {
        self.init(
            red: CGFloat((hex >> 16) & 0xFF) / 255,
            green: CGFloat((hex >> 8) & 0xFF) / 255,
            blue: CGFloat(hex & 0xFF) / 255,
            alpha: 1
        )
    }
}
