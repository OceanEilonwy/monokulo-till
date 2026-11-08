import UserNotifications

/// Whether the system lets Till alert, as the engine's `Action.permissions`
/// takes it (iOS has no bubble). Time-sensitive delivery is each alert's
/// interruption level with its entitlement, not something asked for here.
enum AlertsPermission {
    /// Whether alerts reach the cashier with this authorization.
    static func allows(_ status: UNAuthorizationStatus) -> Bool {
        switch status {
        case .authorized, .provisional, .ephemeral: true
        default: false
        }
    }

    /// The system's answer now.
    nonisolated static func status() async -> UNAuthorizationStatus {
        await UNUserNotificationCenter.current().notificationSettings().authorizationStatus
    }

    /// Asks the cashier, once: iOS shows its question only while the status
    /// is not determined. Whether alerts are allowed afterwards.
    nonisolated static func ask() async -> Bool {
        let granted = try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge])
        return granted ?? false
    }
}
