import Foundation
import Security
import TillCore

/// What the engine asks of the device: secrets in the Keychain, and money
/// written in the device's locale. A Keychain failure is an error, never
/// "no such secret" (docs/decisions/core.md 1, 17): taking it for none
/// would replace a saved connection with nothing.
final class DevicePlatform: Platform, @unchecked Sendable {
    private let service: String

    init(service: String = "com.monokulo.till") {
        self.service = service
    }

    func secretGet(key: String) throws -> Data? {
        var query = base(key)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        switch status {
        case errSecSuccess: return result as? Data
        case errSecItemNotFound: return nil
        default: throw SecretError.Unavailable(reason: Self.describe(status))
        }
    }

    func secretPut(key: String, value: Data) throws {
        let found = SecItemUpdate(base(key) as CFDictionary, [kSecValueData as String: value] as CFDictionary)
        if found == errSecItemNotFound {
            var add = base(key)
            add[kSecValueData as String] = value
            add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
            let added = SecItemAdd(add as CFDictionary, nil)
            guard added == errSecSuccess else { throw SecretError.Unavailable(reason: Self.describe(added)) }
        } else if found != errSecSuccess {
            throw SecretError.Unavailable(reason: Self.describe(found))
        }
    }

    func secretRemove(key: String) throws {
        let status = SecItemDelete(base(key) as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw SecretError.Unavailable(reason: Self.describe(status))
        }
    }

    func formatMoney(minor: Int64, currency: String) -> String {
        TillCore.formatMoney(minor: minor, currency: currency)
    }

    private func base(_ key: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key,
        ]
    }

    private static func describe(_ status: OSStatus) -> String {
        (SecCopyErrorMessageString(status, nil) as String?) ?? "Keychain error \(status)"
    }
}
