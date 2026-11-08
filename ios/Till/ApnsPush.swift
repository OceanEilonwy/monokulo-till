import Foundation
import TillCore
import UIKit

/// The device's push side for the Bell's wake-ups (decisions 19, 25, 44):
/// APNs through the push relay. With `EngineConfig.relayUrl` set the
/// engine carries it itself (design.md §5): it registers the APNs token
/// the app hands over (`Action.pushToken`) with the relay for the Bell's
/// VAPID key, keeps this device's Web Push keys in the shared Keychain, and
/// opens what the relay carries. So this `PushPlatform`, which the engine
/// must be given, is never called on iOS; it answers as APNs would.
final class ApnsPush: PushPlatform, @unchecked Sendable {
    func subscribe(vapidPublicKey: String) {}

    func unsubscribe() {}

    func distributors() throws -> Distributors {
        .found(name: "apns")
    }

    func saveDistributor(name: String) {}

    /// The Live Activity's push token as last handed over, in hex
    /// (`registerLiveActivityPushToken`).
    @MainActor static var liveActivityToken: String?
}

/// Registers for remote notifications as the app starts and hands every
/// APNs token iOS gives (one each launch, a new one when it changes) to the
/// engine, which registers it with the relay (`Action.pushToken`). Alerts
/// being allowed or not doesn't matter here: iOS gives a token either way,
/// and Wake-ups says when alerts are off (decision 25).
@MainActor
final class AppDelegate: NSObject, UIApplicationDelegate {
    /// The app's one core, which SwiftUI draws (`TillApp`). Made on first
    /// use, on the main actor.
    lazy var core = Core()

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        // Made now, so its notification delegate is in place before launch
        // ends, as a tap that launched the app needs.
        _ = core
        application.registerForRemoteNotifications()
        return true
    }

    func application(_ application: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
        core.send(.pushToken(token: RelayedPush.hex(deviceToken)))
    }

    func application(_ application: UIApplication, didFailToRegisterForRemoteNotificationsWithError error: any Error) {
        NSLog("till: no APNs token: %@", error.localizedDescription)
        core.send(.pushToken(token: nil))
    }
}

extension Core {
    /// The hook for the Live Activity's push token (phase I4 calls it with
    /// each token from `Activity.pushTokenUpdates`).
    ///
    /// Not yet sent anywhere: Monokulo pushes a Live Activity through a push
    /// target of kind `live-activity` (design.md §5, "Push targets in
    /// monokulo"), registered with the token's relay endpoint (relay
    /// platform `apns-liveactivity`), and the core has no client for
    /// Monokulo's `/api/v1/push-targets` yet. Until it does, the token is
    /// only kept here.
    func registerLiveActivityPushToken(_ token: Data) {
        ApnsPush.liveActivityToken = RelayedPush.hex(token)
    }
}
