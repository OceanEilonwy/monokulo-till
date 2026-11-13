import TillCore

/// Which of the app's screens draws the engine's `Screen` (docs/ios-plan.md
/// §5): onboarding while setup is on screen, else the till's own screens.
/// Until the first `Update.screen` arrives, the app is starting.
enum Route: Hashable {
    case starting
    case onboarding
    /// The shift, the till's home.
    case shift
    /// One sale: its code while it's live, else its page.
    case sale(orderId: String)
    /// A live sale's code, enlarged for the customer.
    case code(orderId: String)
    /// Every line of a sale's order.
    case order(orderId: String)
    case settings

    static func of(_ screen: Screen?) -> Route {
        guard let screen else { return .starting }
        switch screen {
        case .welcome, .scan, .confirm, .connected, .pointOfSale, .locations, .counter, .alerts, .float,
             .showSales, .ready, .paymentsMode, .walletEntry, .fobRequest, .fobConnected, .walletCode,
             .walletKeys, .walletSeed, .thisTill, .prices, .lightServer:
            return .onboarding
        case .till:
            return .shift
        case .sale(let orderId):
            return .sale(orderId: orderId)
        case .code(let orderId):
            return .code(orderId: orderId)
        case .order(let orderId):
            return .order(orderId: orderId)
        case .settings:
            return .settings
        default:
            // A screen this build doesn't know yet: the till's home.
            return .shift
        }
    }

    /// The till's screens settle after a move, and the engine hears where
    /// (`Action.landed`, decision 55); setup has nothing to settle.
    var lands: Bool {
        switch self {
        case .starting, .onboarding: false
        default: true
        }
    }
}
