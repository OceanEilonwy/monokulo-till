import Foundation
import Testing
@testable import Till

/// Phase I0's checks: the Rust core links, starts and publishes on iOS, and
/// money is written in the device's locale.
@MainActor
struct TillTests {
    @Test func theCoreStartsAndPublishesItsVersion() async throws {
        let core = Core(secrets: MemoryPlatform(), push: ApnsPush())
        for _ in 0..<100 where core.version == nil {
            try await Task.sleep(for: .milliseconds(50))
        }
        let version = try #require(core.version)
        #expect(!version.isEmpty)
    }

    /// The locale's half of money (docs/decisions/core.md 99): its pattern,
    /// with the sign where the core puts the symbol, and its symbols. Which
    /// symbol an amount carries is the core's, tested in Rust.
    @Test func theLocaleGivesItsPatternAndSymbols() {
        let us = Locale(identifier: "en_US")
        #expect(moneyPattern(minor: 2450, currency: "USD", locale: us) == "\u{a4}24.50")
        // Yen have no minor unit: 1200 is ¥1,200, not ¥12.00.
        #expect(moneyPattern(minor: 1200, currency: "JPY", locale: us).contains("1,200"))
        let symbols = currencySymbols(currency: "USD", locale: us)
        #expect(symbols.local == "$")
        #expect(symbols.home == "USD")
    }
}

/// Secrets in memory, so the tests don't touch the simulator's Keychain.
final class MemoryPlatform: Platform, @unchecked Sendable {
    private var secrets: [String: Data] = [:]
    private let lock = NSLock()

    func secretGet(key: String) throws -> Data? { lock.withLock { secrets[key] } }
    func secretPut(key: String, value: Data) throws { lock.withLock { secrets[key] = value } }
    func secretRemove(key: String) throws { _ = lock.withLock { secrets.removeValue(forKey: key) } }
    func formatMoney(minor: Int64, currency: String) -> String { TillCore.moneyPattern(minor: minor, currency: currency) }
    func currencySymbols(currency: String) -> CurrencySymbols { TillCore.currencySymbols(currency: currency) }
}
