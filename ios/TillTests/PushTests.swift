import Foundation
import Testing
import UserNotifications
@testable import Till

/// Phase I5's checks of the glue's pure pieces: the APNs token as the relay
/// takes it, finding the ciphertext in a relayed alert, the alert rewritten
/// in the core's words, and the configuration the app and the extension
/// share. Opening the ciphertext, the relay's registration and the look
/// are the core's, tested in Rust.
struct PushTests {
    @Test func theApnsTokenIsItsBytesInLowercaseHex() {
        #expect(RelayedPush.hex(Data([0x00, 0x0f, 0xa1, 0xff])) == "000fa1ff")
        #expect(RelayedPush.hex(Data()) == "")
    }

    @Test func base64URLWithoutPaddingDecodes() {
        // RFC 8291's auth secret: 16 bytes, so two characters past a group of four.
        let auth = RelayedPush.base64URLDecoded("BTBZMqHH6r4Tts7J_aSIgg")
        #expect(auth?.count == 16)
        #expect(auth?.last == 0x82)
        #expect(RelayedPush.base64URLDecoded("-_8") == Data([0xfb, 0xff]))
        #expect(RelayedPush.base64URLDecoded("AAAA") == Data([0, 0, 0]))
        #expect(RelayedPush.base64URLDecoded("A") == nil, "one character left over is no base64")
        #expect(RelayedPush.base64URLDecoded("**") == nil)
    }

    @Test func onlyARelayedAlertCarriesCiphertext() {
        let relayed: [AnyHashable: Any] = [
            "aps": ["alert": ["title": "New sale"], "mutable-content": 1],
            "aes128gcm": "AAEC",
        ]
        #expect(RelayedPush.ciphertext(relayed) == Data([0, 1, 2]))
        #expect(RelayedPush.ciphertext(["aps": ["alert": "Hi"]]) == nil)
        #expect(RelayedPush.ciphertext(["aes128gcm": 7]) == nil)
        #expect(RelayedPush.ciphertext([SaleAlerts.orderKey: "o1"]) == nil, "the app's own alert")
    }

    @Test func aDingsAlertIsRewrittenInTheCoresWordsAndKeepsItsSaleAndCiphertext() throws {
        let original = UNMutableNotificationContent()
        original.title = "New sale"
        original.userInfo = ["aes128gcm": "AAEC"]
        let alert = SaleAlert(orderId: "ord_1", title: "New Monero sale · $24.50", body: "Tap here to show the payment code.")
        let rewritten = RelayedPush.rewritten(original, as: alert)
        #expect(rewritten.title == "New Monero sale · $24.50")
        #expect(rewritten.body == "Tap here to show the payment code.")
        #expect(rewritten.interruptionLevel == .timeSensitive)
        #expect(rewritten.sound != nil)
        #expect(SaleAlerts.orderId(rewritten.userInfo) == "ord_1", "a tap opens the sale")
        #expect(RelayedPush.ciphertext(rewritten.userInfo) == Data([0, 1, 2]))
        #expect(original.title == "New sale", "the original is left as it was")

        let quiet = RelayedPush.quiet(original)
        #expect(quiet.title == "New sale")
        #expect(quiet.interruptionLevel == .passive)
        #expect(quiet.sound == nil)
    }

    @Test func theAppAndTheExtensionShareTheirConfigurationAndPushThroughTheRelay() {
        let config = TillShared.config(device: .iphone, deviceName: "iPhone")
        #expect(config.relayUrl == "https://relay.monokulo.com")
        #expect(config.bellUrl == "https://bell.monokulo.com")
        #expect(config.platform == "ios")
        #expect(config.dataDir == TillShared.dataDirectory().path)
        #expect(!config.dataDir.isEmpty)
        #expect(SaleAlerts.orderKey == TillShared.orderKey)
    }

    @Test func iosPushPlatformAnswersAsApns() throws {
        #expect(try ApnsPush().distributors() == .found(name: "apns"))
    }
}
