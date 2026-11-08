import Foundation
import TillCore
import UserNotifications

/// A new sale's alert while Till is tucked away (PadTucked, decisions 41
/// and 64): a local notification, time-sensitive so it shows through
/// Focus, in the engine's words. Tapping it opens the sale.
enum SaleAlerts {
    /// Where the alert keeps its sale, for the tap: the same key the
    /// Notification Service Extension writes.
    static let orderKey = TillShared.orderKey

    /// What `alert` posts.
    static func content(_ alert: SaleAlert) -> UNMutableNotificationContent {
        let content = UNMutableNotificationContent()
        content.title = alert.title
        content.body = alert.body
        content.sound = .default
        content.interruptionLevel = .timeSensitive
        content.userInfo = [orderKey: alert.orderId]
        return content
    }

    /// The sale a tapped alert is about.
    static func orderId(_ userInfo: [AnyHashable: Any]) -> String? {
        userInfo[orderKey] as? String
    }

    /// Posts `alert` now, replacing any already posted for its sale.
    static func post(_ alert: SaleAlert) async {
        let request = UNNotificationRequest(identifier: alert.orderId, content: content(alert), trigger: nil)
        try? await UNUserNotificationCenter.current().add(request)
    }

    /// The sale left the shift: its alert goes (`Effect.clearNotification`).
    static func clear(_ id: String) {
        let center = UNUserNotificationCenter.current()
        center.removeDeliveredNotifications(withIdentifiers: [id])
        center.removePendingNotificationRequests(withIdentifiers: [id])
    }
}

/// The notification centre's delegate: a tapped alert opens its sale, and
/// one arriving while Till is in front still shows, but for a push the
/// relay carried (a Bell's ding), which goes to the core instead: Till in
/// front shows the sale itself.
final class NotificationTaps: NSObject, UNUserNotificationCenterDelegate, @unchecked Sendable {
    private let open: @MainActor @Sendable (String) -> Void
    private let relayed: @MainActor @Sendable (Data) -> Void

    init(
        open: @escaping @MainActor @Sendable (String) -> Void,
        relayed: @escaping @MainActor @Sendable (Data) -> Void
    ) {
        self.open = open
        self.relayed = relayed
    }

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse
    ) async {
        guard let orderId = SaleAlerts.orderId(response.notification.request.content.userInfo) else { return }
        await open(orderId)
    }

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification
    ) async -> UNNotificationPresentationOptions {
        if let ciphertext = RelayedPush.ciphertext(notification.request.content.userInfo) {
            await relayed(ciphertext)
            return []
        }
        return [.banner, .list, .sound]
    }
}
