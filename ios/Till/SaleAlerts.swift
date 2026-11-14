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

    /// Posts `alert` now, replacing any already posted for its sale: the
    /// app's own (its identifier is the sale's id) and one the extension
    /// rewrote (`TillShared.deliveredAlerts`). The core posts one alert per
    /// sale (docs/decisions/core.md 123); this keeps it one if it ever didn't.
    static func post(_ alert: SaleAlert) async {
        let center = UNUserNotificationCenter.current()
        let earlier = await TillShared.deliveredAlerts(for: alert.orderId)
        center.removeDeliveredNotifications(withIdentifiers: earlier)
        let request = UNNotificationRequest(identifier: alert.orderId, content: content(alert), trigger: nil)
        try? await center.add(request)
    }

    /// A sale's notice while Till is behind (`Effect.notify`): paid, or its
    /// code ran out unpaid. It replaces the sale's alert, as one sale has
    /// one notification.
    static func notice(orderId: String, _ notice: Notice) async {
        let content = UNMutableNotificationContent()
        content.title = notice.title
        content.body = notice.text ?? ""
        content.sound = .default
        content.userInfo = [orderKey: orderId]
        let center = UNUserNotificationCenter.current()
        let earlier = await TillShared.deliveredAlerts(for: orderId)
        center.removeDeliveredNotifications(withIdentifiers: earlier)
        let request = UNNotificationRequest(identifier: orderId, content: content, trigger: nil)
        try? await center.add(request)
    }

    /// The sale left the shift: its alert goes (`Effect.clearNotification`),
    /// the extension's too.
    /// The till was erased or disconnected: every alert and notice goes, so
    /// none opens a sale of the till that was (core.md decision 264).
    static func clearAll() {
        let center = UNUserNotificationCenter.current()
        center.removeAllDeliveredNotifications()
        center.removeAllPendingNotificationRequests()
    }

    static func clear(_ id: String) {
        let center = UNUserNotificationCenter.current()
        center.removeDeliveredNotifications(withIdentifiers: [id])
        center.removePendingNotificationRequests(withIdentifiers: [id])
        Task {
            let rewritten = await TillShared.deliveredAlerts(for: id)
            UNUserNotificationCenter.current().removeDeliveredNotifications(withIdentifiers: rewritten)
        }
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
