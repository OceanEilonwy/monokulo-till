import Foundation
import TillCore
import UserNotifications

/// What the app and its Notification Service Extension share
/// (docs/ios-plan.md §5): the App Group whose container holds the core's
/// files (the lock around a point of sale's refresh, decision 43), the
/// build's settings, and the push relay. Compiled into both targets.
///
/// The Keychain items are shared through the access group
/// `$(AppIdentifierPrefix)com.monokulo.till.shared`, the first and only
/// entry of both targets' `keychain-access-groups`, so it is where
/// `DevicePlatform` adds them without naming it (the team's prefix is
/// known only once signed).
enum TillShared {
    static let appGroup = "group.com.monokulo.till"

    /// The project's push relay, which turns the APNs token into a Web Push
    /// endpoint for the Bell (design.md §5, "The relay API"): the build's
    /// `TILL_RELAY_URL` (`Build.relayUrl`).
    static var relayUrl: String { Build.relayUrl }

    /// The project's Bell, what "Monokulo Bell" registers with: the
    /// build's `TILL_BELL_URL` (`Build.bellUrl`).
    static var bellUrl: String { Build.bellUrl }

    /// Where a new sale's alert keeps its sale, for the tap: the app's own
    /// alerts and the ones the extension rewrites.
    static let orderKey = "orderId"

    /// The delivered notifications about the sale `orderId`, by the sale
    /// each keeps under `orderKey`: an alert the extension rewrote keeps the
    /// push's own identifier, which the app can't choose, so it's found this
    /// way to be replaced or cleared with the app's (docs/decisions/core.md
    /// 123).
    static func deliveredAlerts(for orderId: String) async -> [String] {
        await UNUserNotificationCenter.current().deliveredNotifications()
            .filter { $0.request.content.userInfo[orderKey] as? String == orderId }
            .map(\.request.identifier)
    }

    /// The App Group's container, where the core keeps its files. An
    /// unsigned build (CI's simulator) may have no group: then the app's
    /// own Application Support.
    ///
    /// Left out of iCloud and computer backups, the core's files and its
    /// ledger (`till.db` and its WAL, which come and go) alike: they're
    /// this device's, as Android's are, and useless without the Keychain
    /// items, which never leave it (docs/decisions/core.md 152).
    static func dataDirectory() -> URL {
        var directory = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: appGroup)
            ?? (try? FileManager.default.url(
                for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true
            )) ?? FileManager.default.temporaryDirectory
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        try? directory.setResourceValues(values)
        return directory
    }

    /// Where Square sends the sign-in back. A sandbox application ends at
    /// the debug page, as Android's debug builds do, since Square takes one
    /// redirect URL per environment and both platforms share the sandbox
    /// application: `ASWebAuthenticationSession` stops at that https address
    /// (the `webcredentials` domain), and should the page load anyway it
    /// forwards an iPhone or iPad to `monokulo-till://square`, which
    /// `IncomingLink` reads back to this address (docs/release.md).
    static func squareRedirect(sandbox: Bool, host: String = Build.linkHost) -> String {
        sandbox ? "https://\(host)/square/debug/" : "https://\(host)/square"
    }

    /// As `squareRedirect`, for Clover's sandbox.
    static func cloverRedirect(environment: CloverEnvironment, host: String = Build.linkHost) -> String {
        environment == .sandbox ? "https://\(host)/clover/debug/" : "https://\(host)/clover"
    }

    /// The Clover environment `name` is, as builds spell them
    /// (`Build.cloverEnvironment`). A build naming none of them can't sign
    /// in to Clover at all, so it stops as it starts.
    static func cloverEnvironment(_ name: String) -> CloverEnvironment {
        switch name {
        case "sandbox": return .sandbox
        case "north_america": return .northAmerica
        case "europe": return .europe
        case "latin_america": return .latinAmerica
        default: preconditionFailure("TillCloverEnvironment isn't a Clover environment: \(name)")
        }
    }

    /// What the engine needs to know about this device and build
    /// (`EngineConfig`, docs/ios-plan.md §2), for the app and the extension
    /// alike, so both read and write the same kept state.
    static func config(device: Device, deviceName: String) -> EngineConfig {
        #if DEBUG
        let debug = true
        #else
        let debug = false
        #endif
        let clover = cloverEnvironment(Build.cloverEnvironment)
        let format = DateFormatter.dateFormat(fromTemplate: "j", options: 0, locale: .current) ?? ""
        return EngineConfig(
            dataDir: dataDirectory().path,
            device: device,
            deviceName: deviceName,
            appVersion: Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0",
            debug: debug,
            utcOffset: Int32(TimeZone.current.secondsFromGMT()),
            hours24: !format.contains("a"),
            square: SquareSetup(
                applicationId: Build.squareApplicationId,
                environment: Build.squareSandbox ? .sandbox : .production,
                redirectUri: squareRedirect(sandbox: Build.squareSandbox),
                baseUrl: nil
            ),
            clover: CloverSetup(
                applicationId: Build.cloverApplicationId,
                environment: clover,
                redirectUri: cloverRedirect(environment: clover),
                baseUrl: nil
            ),
            bellUrl: bellUrl,
            relayUrl: relayUrl
        )
    }
}

/// What a push the relay carried looks like on iOS (design.md §5): an alert
/// with `mutable-content` whose `aes128gcm` key, beside `aps`, holds the
/// ciphertext, base64url. The core opens it (`isRelayedDing`,
/// `Action.pushReceived`); Swift only finds it and hands it over.
enum RelayedPush {
    static let ciphertextKey = "aes128gcm"

    /// The ciphertext a relayed push carries; nil for any other
    /// notification.
    static func ciphertext(_ userInfo: [AnyHashable: Any]) -> Data? {
        guard let text = userInfo[ciphertextKey] as? String else { return nil }
        return base64URLDecoded(text)
    }

    /// Base64url without padding (RFC 4648 §5), as the relay writes it.
    static func base64URLDecoded(_ text: String) -> Data? {
        var base64 = text.replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
        let remainder = base64.count % 4
        if remainder == 1 { return nil }
        if remainder > 0 {
            base64 += String(repeating: "=", count: 4 - remainder)
        }
        return Data(base64Encoded: base64)
    }

    /// An APNs token as the relay takes it: its bytes in lowercase hex.
    static func hex(_ token: Data) -> String {
        token.map { String(format: "%02x", $0) }.joined()
    }

    /// The relay's alert (its fallback "New sale") rewritten as the core's
    /// new-sale alert for `alert`: its words, time-sensitive with a sound,
    /// as the app's own alerts are (`SaleAlerts`), and its sale kept for
    /// the tap. The ciphertext stays, so the app, in front, can still hand
    /// it to the core.
    static func rewritten(_ original: UNNotificationContent, as alert: SaleAlert) -> UNNotificationContent {
        guard let content = original.mutableCopy() as? UNMutableNotificationContent else { return original }
        content.title = alert.title
        content.body = alert.body
        content.sound = .default
        content.interruptionLevel = .timeSensitive
        var userInfo = content.userInfo
        userInfo[TillShared.orderKey] = alert.orderId
        content.userInfo = userInfo
        return content
    }

    /// The relay's alert when a ding found no new sale (the Bell rings for
    /// every payment at the location, a card's too): kept, as iOS requires
    /// something be shown, but passive and silent. Hiding it altogether
    /// needs Apple's notification filtering entitlement.
    static func quiet(_ original: UNNotificationContent) -> UNNotificationContent {
        guard let content = original.mutableCopy() as? UNMutableNotificationContent else { return original }
        content.sound = nil
        content.interruptionLevel = .passive
        return content
    }
}
