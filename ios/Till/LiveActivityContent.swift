import Foundation
import TillCore

/// The engine's `LiveActivityState` as the widget's content state: the
/// same fields, `expiresAt` as a `Date`, nothing decided here (decision 40
/// decides it all in the core).
extension TillActivityAttributes.ContentState {
    init(_ state: LiveActivityState) {
        self.init(
            phase: Phase(state.phase),
            orderId: state.orderId,
            sale: state.sale,
            amount: state.amount,
            currency: state.currency,
            xmr: state.xmr,
            expiresAt: state.expiresAt.map { Date(timeIntervalSince1970: TimeInterval($0)) },
            waitingCount: Int(state.waitingCount),
            paymentUri: state.paymentUri
        )
    }
}

extension TillActivityAttributes.ContentState.Phase {
    init(_ phase: ActivityPhase) {
        switch phase {
        case .idle: self = .idle
        case .waiting: self = .waiting
        case .seen: self = .seen
        case .confirming: self = .confirming
        case .paid: self = .paid
        case .expired: self = .expired
        }
    }
}

extension TillActivityAttributes {
    /// The static attributes from what the engine last published: the
    /// store's name from Settings, the location from the shift's top line.
    init(store: String?, place: String?) {
        self.init(storeName: store ?? "", location: TillActivityAttributes.location(place: place))
    }
}
