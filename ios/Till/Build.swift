import Foundation

/// The build's settings for the points of sale, as Android's Gradle
/// properties are (docs/development.md): the `TILL_*` build settings,
/// which release-ios.yml fills from the testflight environment's variables
/// (docs/release.md), reach here through Info.plist. Both ids are public:
/// the till signs in with PKCE, so no secret is ever in the app. Compiled
/// into the app and the Notification Service Extension, whose Info.plists
/// both carry the keys.
enum Build {
    static let squareApplicationId = value("TillSquareApplicationId") ?? "sq0idp-REPLACE-ME"
    static let squareSandbox = value("TillSquareSandbox") == "true"
    static let cloverApplicationId = value("TillCloverApplicationId") ?? "REPLACE-ME-CLOVER"
    /// `north_america`, `europe`, `latin_america` or `sandbox`.
    static let cloverEnvironment = value("TillCloverEnvironment") ?? "north_america"
    /// The push relay and the Bell the deployment runs (`TILL_RELAY_URL`,
    /// `TILL_BELL_URL`). A build without them gets addresses that never
    /// answer (`.invalid`), as no deployment's are in the source.
    static let relayUrl = value("TillRelayUrl") ?? "https://relay.invalid"
    static let bellUrl = value("TillBellUrl") ?? "https://bell.invalid"
    /// The web host of the app's universal links and the points of sale's
    /// sign-in redirects (`TILL_LINK_HOST`), which the associated domains
    /// name too.
    static let linkHost = value("TillLinkHost") ?? "till.invalid"

    private static func value(_ key: String) -> String? {
        guard let value = Bundle.main.object(forInfoDictionaryKey: key) as? String,
              !value.isEmpty, !value.hasPrefix("$(") else { return nil }
        return value
    }
}
