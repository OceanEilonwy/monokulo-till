import CoreFoundation
import Foundation
import TillCore

/// What only iOS can do for the core's ledger (docs/ledger.md §11; docs/
/// decisions/core.md 151, 152). Compiled into the app and its Notification
/// Service Extension. Nothing here decides anything about the ledger: the
/// core says when it writes, and decides what to close, reopen or re-read.
///
/// iOS kills a suspended process that holds a lock on a file in the shared
/// container unless it sees an SQLite WAL database there (0xdead10cc), so
/// each write the core makes runs under a background task assertion it asks
/// for through `BackgroundWork`, and the app tells the engine before it may
/// be suspended (`Engine.suspend`).
final class DeviceBackgroundWork: BackgroundWork, @unchecked Sendable {
    static let shared = DeviceBackgroundWork()

    private let lock = NSLock()
    private var next: UInt64 = 0
    private var held: [UInt64: DispatchSemaphore] = [:]

    /// Holds the process awake until `end(id:)`, through
    /// `performExpiringActivity`, which the app and its extensions alike may
    /// use from any thread. Its block holds the assertion while it runs, so
    /// it waits for the end; told the time is up, it lets go, and the core's
    /// short transaction is left to finish or roll back.
    func begin(reason: String) -> UInt64 {
        let release = DispatchSemaphore(value: 0)
        let id = lock.withLock {
            next += 1
            held[next] = release
            return next
        }
        ProcessInfo.processInfo.performExpiringActivity(withReason: reason) { expired in
            if expired {
                release.signal()
            } else {
                release.wait()
            }
        }
        return id
    }

    func end(id: UInt64) {
        let release = lock.withLock { held.removeValue(forKey: id) }
        release?.signal()
    }
}

/// The Darwin notification the extension posts after each round, which may
/// have written the ledger, and the app hears: the engine re-reads it
/// (`Engine.ledgerChanged`). It carries nothing but its name.
enum LedgerSignal {
    static let name = "com.monokulo.till.ledger-changed"

    static func post() {
        CFNotificationCenterPostNotification(
            CFNotificationCenterGetDarwinNotifyCenter(),
            CFNotificationName(name as CFString), nil, nil, true
        )
    }
}

/// Hears `LedgerSignal` until released, calling `changed` on the main thread.
final class LedgerSignalObserver: @unchecked Sendable {
    private let changed: @Sendable () -> Void

    init(_ changed: @escaping @Sendable () -> Void) {
        self.changed = changed
        CFNotificationCenterAddObserver(
            CFNotificationCenterGetDarwinNotifyCenter(),
            Unmanaged.passUnretained(self).toOpaque(),
            { _, observer, _, _, _ in
                guard let observer else { return }
                Unmanaged<LedgerSignalObserver>.fromOpaque(observer).takeUnretainedValue().changed()
            },
            LedgerSignal.name as CFString, nil, .deliverImmediately
        )
    }

    deinit {
        CFNotificationCenterRemoveEveryObserver(
            CFNotificationCenterGetDarwinNotifyCenter(),
            Unmanaged.passUnretained(self).toOpaque()
        )
    }
}

/// Tells the engine when the app leaves the screen and comes back, in
/// order on a queue of its own: `suspend` blocks until the engine has
/// closed what it holds, under a background task assertion, off the main
/// thread.
final class EngineLifecycle: @unchecked Sendable {
    private let queue = DispatchQueue(label: "com.monokulo.till.lifecycle")

    func suspend(_ engine: Engine) {
        let work = DeviceBackgroundWork.shared
        let hold = work.begin(reason: "Till closes its ledger")
        queue.async {
            engine.suspend()
            work.end(id: hold)
        }
    }

    func resume(_ engine: Engine) {
        queue.async { engine.resume() }
    }
}
