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

    /// The window's shape: a square for a sale's code, else the strip: a
    /// sale whose payment is seen has no code left to show, so it is the
    /// strip too (the design's P1 to P4).
    var shape: PipShape {
        switch self {
        case .sale(let sale): sale.code == nil ? .strip : .square
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

    /// "Seen" or "Confirming", the strip's first line once a payment is seen.
    var progressWord: String {
        progress?.components(separatedBy: " ").first ?? "Seen"
    }

    /// "14:32 left", from `now`; nil without a countdown.
    func timeLeft(now: Date) -> String? {
        guard let expiresAt else { return nil }
        let seconds = max(0, Int(expiresAt.timeIntervalSince(now).rounded(.up)))
        return String(format: "%d:%02d left", seconds / 60, seconds % 60)
    }
}

/// A cash sale offered in Monero, as the engine publishes it for the
/// window (`ShiftView.offeredCash`, core.md decision 128): this counter's
/// newest not kept as cash, whatever the list's filter or counter shows.
struct PipCashOffer: Equatable, Sendable {
    let orderId: String
    /// "Sam · $24.50".
    let title: String
    /// The strip's second line: "Cash · paying in Monero?".
    let line: String

    /// The engine's offered cash sale, if any.
    static func of(_ offered: OfferedCash?) -> PipCashOffer? {
        offered.map { PipCashOffer(orderId: $0.orderId, title: $0.title, line: $0.line) }
    }
}

/// The window's two shapes (P4): Till chooses the shape, iOS the size.
enum PipShape: Equatable, Sendable {
    /// Under 3:1 (124 × 44 pt by default), small and out of the way.
    case strip
    /// 1:1, for a sale's code.
    case square

    /// Width over height.
    var aspect: CGFloat {
        switch self {
        case .strip: 2.8
        case .square: 1
        }
    }

    /// The window's width in points until iOS says: the strip small, the
    /// square big enough to scan.
    var defaultWidth: CGFloat {
        switch self {
        case .strip: 124
        case .square: 200
        }
    }

    /// The frames' size in points, for a window `width` points wide (what
    /// iOS last said, else the shape's default): edge to edge, with
    /// the shape's aspect, which is how iOS knows the shape.
    func size(width: CGFloat?) -> CGSize {
        let width = max(PipShape.smallest, (width ?? defaultWidth).rounded())
        return CGSize(width: width, height: (width / aspect).rounded())
    }

    static let smallest: CGFloat = 100

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
