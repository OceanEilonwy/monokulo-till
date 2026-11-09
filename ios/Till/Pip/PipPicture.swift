import CoreGraphics
import Foundation
import TillCore

/// What the iPad's Picture in Picture window shows (docs/ipad-redesign.md
/// decisions 2 and 8; boards P1–P4, K1), chosen from what the engine
/// publishes: the Live Activity's state, which the engine sends on iPad
/// while a new sale shows up in a floating window, and the shift's cash
/// sales offered in Monero. Nothing is decided here that the engine
/// already said: another counter's sales are never in either.
enum PipPicture: Equatable, Sendable {
    /// P1: the Till logo, idle and dark, "Watching" and the location.
    case watching(place: String)
    /// P2: whom it's for, the code while it waits, the amount and the time
    /// left. Seen and confirming show how far the payment is instead.
    case sale(PipSale)
    /// P3: the green Till logo, "Paid · $24.50" and whom it was for; the
    /// engine eases back to watching after its paid dwell.
    case paid(amount: String, name: String)
    /// K1: a cash sale offered in Monero ("Only offer Monero" on). The
    /// system's play control takes it in Monero.
    case cash(PipCashOffer)

    /// The window's shape: a square for a sale's code, else the strip.
    var shape: PipShape {
        switch self {
        case .sale: .square
        case .watching, .paid, .cash: .strip
        }
    }

    /// The sale ⤢ opens full screen (A2), if the window shows one.
    var orderId: String? {
        switch self {
        case .sale(let sale): sale.orderId
        case .cash(let offer): offer.orderId
        case .watching, .paid: nil
        }
    }

    /// The cash sale the play control takes in Monero, while one shows.
    var offer: PipCashOffer? {
        if case .cash(let offer) = self { return offer }
        return nil
    }

    /// The picture for the engine's `state` (nil: the window is off),
    /// `cash` offered on the shift, and the till's `place`. Paid comes
    /// first, then a live sale, then a cash sale offered, then watching;
    /// a code that ran out goes back to watching, as the strip has no
    /// words for it (the shift lists it).
    static func of(
        _ state: TillActivityAttributes.ContentState?,
        cash: PipCashOffer?,
        place: String,
        locale: Locale = .autoupdatingCurrent
    ) -> PipPicture {
        let watching = PipPicture.watching(place: place)
        guard let state else { return watching }
        let amount = state.money(locale: locale) ?? ""
        let name = PipPicture.name(state)
        switch state.phase {
        case .paid:
            return .paid(amount: amount, name: name)
        case .waiting, .seen, .confirming:
            guard let orderId = state.orderId else { return cash.map(PipPicture.cash) ?? watching }
            return .sale(PipSale(
                orderId: orderId,
                name: name,
                amount: amount,
                expiresAt: state.phase == .waiting ? state.expiresAt : nil,
                code: state.phase == .waiting ? state.paymentUri : nil,
                progress: state.phase == .waiting ? nil : state.status(location: "")
            ))
        case .idle, .expired:
            return cash.map(PipPicture.cash) ?? watching
        }
    }

    /// Whom the sale is for, else its number ("Sale #1042"), as rows say.
    static func name(_ state: TillActivityAttributes.ContentState) -> String {
        if let name = state.name?.trimmingCharacters(in: .whitespaces), !name.isEmpty {
            return name
        }
        if let sale = state.sale, !sale.isEmpty {
            return "Sale \(sale)"
        }
        return ""
    }
}

/// A live sale in the square window.
struct PipSale: Equatable, Sendable {
    let orderId: String
    let name: String
    /// "$24.50".
    let amount: String
    /// While it waits: when the code runs out.
    let expiresAt: Date?
    /// While it waits: the payment URI the code draws.
    let code: String?
    /// Once a payment is seen: "Seen on the network", "Confirming".
    let progress: String?

    /// "14:32 left", from `now`; nil without a countdown.
    func timeLeft(now: Date) -> String? {
        guard let expiresAt else { return nil }
        let seconds = max(0, Int(expiresAt.timeIntervalSince(now).rounded(.up)))
        return String(format: "%d:%02d left", seconds / 60, seconds % 60)
    }
}

/// A cash sale offered in Monero, from the shift's cash section (core.md
/// decision 116): its order id is `cash:` and the payment's id until it's
/// taken.
struct PipCashOffer: Equatable, Sendable {
    let orderId: String
    /// "Sam · $24.50".
    let title: String

    /// The section's header, which the strip's second line says.
    static let line = "Cash · paying in Monero?"

    /// The first of this counter's cash sales offered in Monero on the
    /// shift, if any. The engine draws them under their own header (an
    /// `Earlier` entry); kept-as-cash sales aren't offered, and another
    /// counter's never shows in the window (decision 5).
    static func first(in entries: [ShiftEntry]) -> PipCashOffer? {
        var inCash = false
        for entry in entries {
            if case .row(let orderId, let row, _, _, _) = entry {
                if inCash && orderId.hasPrefix("cash:") && !row.otherCounter {
                    return PipCashOffer(orderId: orderId, title: row.title)
                }
            } else if case .earlier(let label) = entry {
                inCash = label == line
            } else {
                inCash = false
            }
        }
        return nil
    }
}

/// The window's two shapes (P4): Till chooses the shape, iOS the size.
enum PipShape: Equatable, Sendable {
    /// About 3:1, out of the way while watching.
    case strip
    /// 1:1, for a sale's code.
    case square

    /// Width over height.
    var aspect: CGFloat {
        switch self {
        case .strip: 3
        case .square: 1
        }
    }

    /// The frames' size in points, for a window `width` points wide (what
    /// iOS last said, else a medium window's 240): edge to edge, with
    /// the shape's aspect, which is how iOS knows the shape.
    func size(width: CGFloat?) -> CGSize {
        let width = max(PipShape.smallest, (width ?? PipShape.medium).rounded())
        return CGSize(width: width, height: (width / aspect).rounded())
    }

    static let smallest: CGFloat = 120
    static let medium: CGFloat = 240

    /// Below about 200 pt a phone's camera can't read the code reliably
    /// (P4), so the window shows the amount and "Tap ⤢ to show the
    /// customer" instead.
    static func codeFits(width: CGFloat) -> Bool {
        width >= 200
    }

    /// A window's width in points from the render size iOS reports in
    /// pixels (`didTransitionToRenderSize`).
    static func width(renderWidth: Int32, scale: CGFloat) -> CGFloat {
        CGFloat(renderWidth) / max(scale, 1)
    }
}
