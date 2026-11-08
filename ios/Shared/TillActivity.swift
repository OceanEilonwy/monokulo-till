import ActivityKit
import Foundation

/// The Live Activity's attributes (docs/design.md §6), compiled into both
/// the app and the widget extension, which doesn't link the core
/// (docs/ios-plan.md §5). The app fills them from the engine's
/// `LiveActivityState` (`LiveActivityContent.swift`); Monokulo's pushes,
/// through the relay, send the same content state as JSON.
struct TillActivityAttributes: ActivityAttributes, Sendable {
    /// What changes: the sale shown and how it stands.
    struct ContentState: Codable, Hashable, Sendable {
        /// The activity's layout and colour.
        enum Phase: String, Codable, Hashable, Sendable {
            case idle, waiting, seen, confirming, paid, expired
        }

        var phase: Phase
        /// The order shown, for the tap's `monokulo-till://sale/{id}`. Set by
        /// the app; Monokulo's pushes don't carry it.
        var orderId: String?
        /// "#1042".
        var sale: String?
        /// "24.50", written with `currency` by the device (``money(locale:)``).
        var amount: String?
        var currency: String?
        /// "0.1421".
        var xmr: String?
        /// When the code runs out: the countdown iOS draws, and the stale date.
        var expiresAt: Date?
        /// The till's own sales still waiting, this one included while it is.
        var waitingCount: Int
        /// The plain payment URI, only with "Show code in island" on.
        var paymentUri: String?

        /// The amount in the device's way of writing money: "$24.50".
        func money(locale: Locale = .autoupdatingCurrent) -> String? {
            guard let amount else { return nil }
            guard let currency, !currency.isEmpty,
                  let value = Decimal(string: amount, locale: Locale(identifier: "en_US_POSIX"))
            else { return amount }
            return value.formatted(Decimal.FormatStyle.Currency(code: currency, locale: locale))
        }

        /// "+1 more" while another of the till's sales waits too.
        var more: String? {
            waitingCount > 1 ? "+\(waitingCount - 1) more" : nil
        }

        /// The line over the status (PhoneOrder, PhonePaid): "New sale ·
        /// #1042 · +1 more", "Paid · sale #1042"; nil while idle.
        var headline: String? {
            let label = sale ?? ""
            switch phase {
            case .idle:
                return nil
            case .waiting:
                return (["New sale", label] + [more].compactMap { $0 }).filter { !$0.isEmpty }.joined(separator: " · ")
            case .paid:
                return label.isEmpty ? "Paid" : "Paid · sale \(label)"
            case .seen, .confirming, .expired:
                return label.isEmpty ? "Sale" : "Sale \(label)"
            }
        }

        /// The status, as design.md §6's table says it: "Watching for sales
        /// · Front Counter", "Waiting for payment", "Seen on the network",
        /// "Confirming", "0.1421 XMR received", "No payment yet · refund in
        /// Square".
        func status(location: String) -> String {
            switch phase {
            case .idle:
                return location.isEmpty ? "Watching for sales" : "Watching for sales · \(location)"
            case .waiting: return "Waiting for payment"
            case .seen: return "Seen on the network"
            case .confirming: return "Confirming"
            case .paid: return xmr.map { "\($0) XMR received" } ?? "Paid"
            case .expired: return "No payment yet · refund in Square"
            }
        }

        /// The compact island's trailing words: the amount, "Seen" or "1/2";
        /// nil while idle (a green dot).
        var compact: String? {
            switch phase {
            case .idle: nil
            case .seen: "Seen"
            case .confirming: "1/2"
            case .waiting, .paid, .expired: money()
            }
        }

        /// Under the status while a code is up: the island's code enlarges in
        /// the app; without one, the app shows it.
        var hint: String? {
            guard phase == .waiting else { return nil }
            return paymentUri == nil ? "Tap to show code" : "Tap to enlarge"
        }

        /// When iOS should mark the activity out of date: a live sale's
        /// code running out. Idle, paid and expired never go stale.
        var staleDate: Date? {
            switch phase {
            case .waiting, .seen, .confirming: expiresAt
            case .idle, .paid, .expired: nil
            }
        }
    }

    /// "The Bean Room".
    var storeName: String
    /// "Front Counter": the idle line's "Watching for sales · Front Counter".
    var location: String

    /// The location from the shift's top line, which says the store and
    /// the location ("The Bean Room · Front Counter"), or one of them once
    /// when they're the same.
    static func location(place: String?) -> String {
        guard let place else { return "" }
        let parts = place.components(separatedBy: " · ")
        return parts.last?.trimmingCharacters(in: .whitespaces) ?? ""
    }
}

/// The relay's JSON for the content state (design.md §6): `paymentURI`,
/// and `expiresAt` in Unix seconds, where `Date`'s own coding would count
/// from 2001.
extension TillActivityAttributes.ContentState {
    enum CodingKeys: String, CodingKey {
        case phase, orderId, sale, amount, currency, xmr, expiresAt, waitingCount
        case paymentUri = "paymentURI"
    }

    init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.init(
            phase: try container.decodeIfPresent(Phase.self, forKey: .phase) ?? .idle,
            orderId: try container.decodeIfPresent(String.self, forKey: .orderId),
            sale: try container.decodeIfPresent(String.self, forKey: .sale),
            amount: try container.decodeIfPresent(String.self, forKey: .amount),
            currency: try container.decodeIfPresent(String.self, forKey: .currency),
            xmr: try container.decodeIfPresent(String.self, forKey: .xmr),
            expiresAt: try container.decodeIfPresent(Double.self, forKey: .expiresAt)
                .map { Date(timeIntervalSince1970: $0) },
            waitingCount: try container.decodeIfPresent(Int.self, forKey: .waitingCount) ?? 0,
            paymentUri: try container.decodeIfPresent(String.self, forKey: .paymentUri)
        )
    }

    func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(phase, forKey: .phase)
        try container.encodeIfPresent(orderId, forKey: .orderId)
        try container.encodeIfPresent(sale, forKey: .sale)
        try container.encodeIfPresent(amount, forKey: .amount)
        try container.encodeIfPresent(currency, forKey: .currency)
        try container.encodeIfPresent(xmr, forKey: .xmr)
        try container.encodeIfPresent(expiresAt?.timeIntervalSince1970, forKey: .expiresAt)
        try container.encode(waitingCount, forKey: .waitingCount)
        try container.encodeIfPresent(paymentUri, forKey: .paymentUri)
    }
}

/// The link a tap on the activity opens: `monokulo-till://sale/{id}`, or
/// `monokulo-till://sale` when the state doesn't name its order (after a
/// push), which only brings Till forward.
enum TillActivityLink {
    static let scheme = "monokulo-till"
    static let host = "sale"

    static func url(orderId: String?) -> URL {
        var parts = URLComponents()
        parts.scheme = scheme
        parts.host = host
        if let orderId, !orderId.isEmpty {
            parts.path = "/" + orderId
        }
        return parts.url ?? URL(string: "\(scheme)://\(host)")!
    }

    /// Whether `url` is the activity's link, with an order or without.
    static func isSale(_ url: URL) -> Bool {
        let parts = URLComponents(url: url, resolvingAgainstBaseURL: false)
        return parts?.scheme?.lowercased() == scheme && parts?.host?.lowercased() == host
    }

    /// The order the activity's link names, if any.
    static func orderId(_ url: URL) -> String? {
        guard isSale(url), let path = URLComponents(url: url, resolvingAgainstBaseURL: false)?.path else { return nil }
        let id = path.hasPrefix("/") ? String(path.dropFirst()) : path
        return id.isEmpty ? nil : id
    }
}
