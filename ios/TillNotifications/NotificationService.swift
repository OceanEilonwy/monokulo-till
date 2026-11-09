@preconcurrency import UserNotifications
import Foundation
import TillCore
import UIKit

/// The Notification Service Extension (design.md §3, docs/ios-plan.md §5):
/// a Bell's ding comes through the push relay as an alert with
/// `mutable-content`, its ciphertext beside `aps`, saying only "New sale".
/// The extension asks the core whether it is a ding for this till
/// (`isRelayedDing`), looks for the sale once (`backgroundCheck`: Square or
/// Clover asked, the Monokulo order made, the shift and cursor kept in the
/// shared Keychain, the sign-in's refresh under the lock in the App
/// Group's container), and rewrites the alert in the core's words, with
/// the sale kept for the tap.
///
/// Memory is about 24 MB here: the core runs no engine, only a
/// current-thread runtime for the one look (decision 45). The look gets
/// most of the extension's 30 s; if iOS calls time first, the alert goes
/// out as the relay sent it, and the app does the rest when it opens.
final class NotificationService: UNNotificationServiceExtension {
    /// How long the look may take, of the extension's 30 s.
    static let deadlineMs: UInt64 = 25_000

    private var delivery: Delivery?

    override func didReceive(
        _ request: UNNotificationRequest,
        withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void
    ) {
        let delivery = Delivery(original: request.content, handler: contentHandler)
        self.delivery = delivery
        guard let ciphertext = RelayedPush.ciphertext(request.content.userInfo) else {
            delivery.deliver(nil)
            return
        }
        Task { @MainActor in
            let device: Device = UIDevice.current.userInterfaceIdiom == .pad ? .ipad : .iphone
            // The look blocks its thread: never the main one.
            DispatchQueue.global(qos: .userInitiated).async {
                delivery.deliver(
                    NotificationService.look(ciphertext: ciphertext, device: device, original: delivery.original)
                )
            }
        }
    }

    override func serviceExtensionTimeWillExpire() {
        delivery?.deliver(nil)
    }

    /// What the alert should say after one look; nil keeps the relay's.
    private static func look(ciphertext: Data, device: Device, original: UNNotificationContent) -> UNNotificationContent? {
        let platform = DevicePlatform()
        guard isRelayedDing(platform: platform, content: ciphertext) else { return nil }
        let config = TillShared.config(device: device, deviceName: "")
        // The core holds the extension awake around each ledger write, and
        // the app re-reads the ledger after the round (CoreLifecycle.swift).
        setBackgroundWork(work: DeviceBackgroundWork.shared)
        let check = backgroundCheck(config: config, platform: platform, deadlineMs: deadlineMs)
        LedgerSignal.post()
        if let alert = check.alerts.first {
            return RelayedPush.rewritten(original, as: alert)
        }
        if check.finished && check.problem == nil {
            return RelayedPush.quiet(original)
        }
        if let problem = check.problem {
            NSLog("till-notifications: %@", problem)
        }
        return nil
    }
}

/// The content handler, called once: by the look, or by iOS's time running
/// out, whichever comes first.
private final class Delivery: @unchecked Sendable {
    let original: UNNotificationContent
    private var handler: ((UNNotificationContent) -> Void)?
    private let lock = NSLock()

    init(original: UNNotificationContent, handler: @escaping (UNNotificationContent) -> Void) {
        self.original = original
        self.handler = handler
    }

    /// Hands iOS `content`, or the original when nil.
    func deliver(_ content: UNNotificationContent?) {
        let handler = lock.withLock {
            let handler = self.handler
            self.handler = nil
            return handler
        }
        handler?(content ?? original)
    }
}
