import SwiftUI
import TillCore

/// The engine's colour roles (`Tone`, `Ground`, `StateIcon`) as the
/// palette's colours, as Android's `toneColor`, `groundColor` and
/// `StateBadge` read them, for the till's words and notes.
enum Roles {
    /// Words in `tone`: a row's line, the shift's state word.
    static func ink(_ tone: Tone) -> Color {
        switch tone {
        case .plain: Palette.ink
        case .muted: Palette.secondary
        case .paid: Palette.paid
        case .warn: Palette.expiredInk
        case .link: Palette.link
        case .danger: Palette.danger
        }
    }

    /// A page's hero or note on `ground`.
    static func ground(_ ground: Ground) -> Color {
        switch ground {
        case .paid: Palette.paidGround
        case .warn: Palette.expiredGround
        case .danger: Palette.dangerGround
        case .chip: Palette.chip
        case .card: Palette.card
        }
    }

    /// Text on `ground`: the reversed payment's note is dark red on pink.
    static func onGround(_ ground: Ground) -> Color {
        switch ground {
        case .danger: Palette.onDangerGround
        case .warn: Palette.expiredInk
        default: Palette.ink
        }
    }

    /// A shift row's badge ground, by its glyph.
    static func badgeGround(_ icon: StateIcon) -> Ground {
        switch icon {
        case .clock: .warn
        case .warning: .danger
        case .tick: .paid
        case .ring, .refund, .close, .cash: .chip
        }
    }

    /// The SF Symbol for `icon`; nil for the ring, which is drawn.
    static func symbol(_ icon: StateIcon) -> String? {
        switch icon {
        case .ring: nil
        case .clock: "clock"
        case .warning: "exclamationmark.triangle.fill"
        case .tick: "checkmark"
        case .refund: "arrow.uturn.backward"
        case .close: "xmark"
        case .cash: "banknote"
        }
    }

    /// The glyph's colour; `quiet` mutes a refunded or closed sale's.
    static func glyph(_ icon: StateIcon, quiet: Bool) -> Color {
        switch icon {
        case .clock: Palette.expiredInk
        case .warning: Palette.danger
        case .tick: Palette.paid
        case .ring, .refund, .close, .cash: quiet ? Palette.secondary : Palette.ink
        }
    }
}
