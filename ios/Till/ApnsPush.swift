import TillCore

/// The device's push side for the Bell's wake-ups (`PushPlatform`). On iOS
/// the only distributor is APNs, through the relay (docs/decisions/core.md
/// 19, 25). Registering with APNs and the relay comes in phase I5; until
/// then nothing subscribes, and the till works on its timer alone.
final class ApnsPush: PushPlatform, @unchecked Sendable {
    func subscribe(vapidPublicKey: String) {}

    func unsubscribe() {}

    func distributors() throws -> Distributors {
        .found(name: "apns")
    }

    func saveDistributor(name: String) {}
}
