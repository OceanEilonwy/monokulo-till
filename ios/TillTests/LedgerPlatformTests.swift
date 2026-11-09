import Foundation
import Testing
@testable import Till

/// What iOS gives the core's ledger (docs/decisions/core.md 150-152):
/// SQLCipher in the library, background task assertions, the extension's
/// Darwin notification, and no backup of the core's files.
struct LedgerPlatformTests {
    /// The library carries SQLCipher, built with CommonCrypto, and it
    /// encrypts and reads back as the ledger will use it.
    @Test func theCoreCarriesSqlcipher() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let version = try sqlcipherCheck(dir: dir.path)
        #expect(version.hasPrefix("4"))
    }

    @Test func aBackgroundHoldBeginsAndEnds() {
        let work = DeviceBackgroundWork()
        let first = work.begin(reason: "test")
        let second = work.begin(reason: "test")
        #expect(first != second)
        work.end(id: first)
        work.end(id: second)
        // Ending twice is harmless.
        work.end(id: first)
    }

    @Test @MainActor func theAppHearsTheExtensionsSignal() async throws {
        final class Heard: @unchecked Sendable {
            var count = 0
        }
        let heard = Heard()
        let observer = LedgerSignalObserver { heard.count += 1 }
        LedgerSignal.post()
        for _ in 0..<50 where heard.count == 0 {
            try await Task.sleep(for: .milliseconds(20))
        }
        #expect(heard.count >= 1)
        withExtendedLifetime(observer) {}
    }

    @Test func theCoresFilesAreLeftOutOfBackups() throws {
        let values = try TillShared.dataDirectory().resourceValues(forKeys: [.isExcludedFromBackupKey])
        #expect(values.isExcludedFromBackup == true)
    }
}
