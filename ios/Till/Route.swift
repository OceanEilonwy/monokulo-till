import TillCore

/// Which of the app's screens draws the engine's `Screen` (docs/ios-plan.md
/// §5): onboarding while setup is on screen, the till otherwise. Until the
/// first `Update.screen` arrives, the app is starting.
enum Route: Equatable {
    case starting
    case onboarding
    /// The till: phase I0's core page until I3 and I4 draw the shift.
    case till

    static func of(_ screen: Screen?) -> Route {
        guard let screen else { return .starting }
        switch screen {
        case .welcome, .scan, .confirm, .connected, .pointOfSale, .locations, .alerts, .float, .ready:
            return .onboarding
        default:
            return .till
        }
    }
}
