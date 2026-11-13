import SwiftUI
import UIKit

/// The design's colours by role (docs/ios-plan.md §4; docs/ipad-redesign.md
/// "Look" and the Tokens board). Dark mode is OLED black, matching Fob's
/// OledColors (as Android's DarkTokens): a black ground, near-black cards,
/// one warm line for dividers and edges, and the grounds behind warn,
/// danger and paid kept near black too. Every text pair is at least 4.5:1,
/// and Increase Contrast darkens (in dark mode, lightens) the secondary
/// text, paid and progress.
enum Palette {
    /// The page.
    static let ground = Color.adaptive(light: 0xF4F1EA, dark: 0x000000)
    /// Text and icons.
    static let ink = Color.adaptive(light: 0x1A1917, dark: 0xF2EEE6, darkContrast: 0xFFFFFF)
    /// Secondary text.
    static let secondary = Color.adaptive(light: 0x5A564C, dark: 0xB0A999, lightContrast: 0x3F3B33, darkContrast: 0xC9C3B8)
    /// Cards and grouped rows (the boards' "receipt"). The code's card
    /// stays white in both: ``codeGround``.
    static let card = Color.adaptive(light: 0xFFFFFF, dark: 0x0C0B0A)
    /// The one primary button, and nothing else, with ``onAccent`` on it:
    /// the same in both themes.
    static let accent = Color(hex: 0xFF6600)
    static let onAccent = Color(hex: 0x111111)
    /// A payment's progress, the bar's fill: 3.5:1 on white (the Till
    /// logo's second facet shade), Monero orange on dark.
    static let progress = Color.adaptive(light: 0xE85D00, dark: 0xFF6600, lightContrast: 0xC24E00, darkContrast: 0xFF8A3D)
    /// The progress bar's track: 3.6:1 on white; in dark, Fob's warm line
    /// (the outline's shade with Increase Contrast).
    static let track = Color.adaptive(light: 0x8C8577, dark: 0x2A2723, lightContrast: 0x6E6A63, darkContrast: 0x6B655A)
    /// Text links, and only things that can be tapped.
    static let link = Color.adaptive(light: 0xB34700, dark: 0xFF8F45)
    /// Paid, as text and ticks: paid and nothing else.
    static let paid = Color.adaptive(light: 0x1A7F37, dark: 0x5CC88A, lightContrast: 0x146C2E, darkContrast: 0x6FD496)
    /// Disconnect; a reversed payment.
    static let danger = Color.adaptive(light: 0xB00020, dark: 0xFF6B6B)
    /// Lines between rows inside a card.
    static let hairline = Color.adaptive(light: 0xECE6DA, dark: 0x2A2723)
    /// A card's or a quiet button's edge.
    static let border = Color.adaptive(light: 0xDCD5C7, dark: 0x2A2723)
    /// An expired code's note: its ground and its text; the warn colour.
    static let expiredGround = Color.adaptive(light: 0xF1E3CF, dark: 0x3A2814)
    static let expiredInk = Color.adaptive(light: 0x7D5E00, dark: 0xE8C267)
    /// A paid sale's badge and page hero ground (Android's `paidGround`).
    static let paidGround = Color.adaptive(light: 0xDCEFE2, dark: 0x10291A)
    /// A reversed payment's note: its ground and its text.
    static let dangerGround = Color.adaptive(light: 0xF6DCD9, dark: 0x331614)
    static let onDangerGround = Color.adaptive(light: 0x6E1A1A, dark: 0xFFD2CE)
    /// A quiet badge's ground, and an unselected filter chip's (Android's
    /// `chip`); sunken and inset fills.
    static let chip = Color.adaptive(light: 0xE4DFD5, dark: 0x181613)
    /// The sale open beside the sidebar, its row's fill (the DesignPad
    /// board's #FBE6D6): a pale wash of the accent, under ink text that
    /// stays 4.5:1.
    static let selection = Color.adaptive(light: 0xFBE6D6, dark: 0x3A210F)
    /// A secondary button's outline (the boards' 1.5 pt #8C8577 edge).
    static let outline = Color.adaptive(light: 0x8C8577, dark: 0x6B655A)

    /// The QR code's modules and card, ink on white whatever the theme.
    static let codeInk = Color(hex: 0x1A1917)
    static let codeGround = Color.white

    /// Which of a colour's variants shows, by appearance and contrast.
    static func hex(dark: Bool, contrast: Bool, light: UInt32, darkHex: UInt32, lightContrast: UInt32?, darkContrast: UInt32?) -> UInt32 {
        if dark {
            return contrast ? (darkContrast ?? darkHex) : darkHex
        }
        return contrast ? (lightContrast ?? light) : light
    }
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

    /// A colour that follows the system's light or dark appearance, and
    /// Increase Contrast where a stronger variant is given.
    static func adaptive(light: UInt32, dark: UInt32, lightContrast: UInt32? = nil, darkContrast: UInt32? = nil) -> Color {
        Color(uiColor: UIColor { traits in
            UIColor(hex: Palette.hex(
                dark: traits.userInterfaceStyle == .dark,
                contrast: traits.accessibilityContrast == .high,
                light: light,
                darkHex: dark,
                lightContrast: lightContrast,
                darkContrast: darkContrast
            ))
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
