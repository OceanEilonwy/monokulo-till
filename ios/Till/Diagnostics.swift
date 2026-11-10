import Foundation
import os

/// What the app writes to the unified log for a crash to be read by: the
/// floating window coming and going, the app's memory as it goes to the
/// back, memory warnings and background holds that ran out. Written at the
/// default (`notice`) level or above, which the system keeps on disk, so a
/// sysdiagnose or Console taken after a crash shows the run up to it.
/// Read it with Console, filtering on the subsystem `com.monokulo.till`.
enum TillLog {
    static let subsystem = "com.monokulo.till"
    static let pip = Logger(subsystem: subsystem, category: "pip")
    static let lifecycle = Logger(subsystem: subsystem, category: "lifecycle")
    static let memory = Logger(subsystem: subsystem, category: "memory")
}

/// How much more memory the app may use before iOS ends it
/// (`os_proc_available_memory`, against the same limit Jetsam applies), in
/// megabytes. Logged as the app goes to the back and on memory warnings:
/// a number that falls run after run is a leak; one near zero as Square
/// comes to the front is the floating window's app being ended for memory.
enum MemoryHeadroom {
    static var megabytes: Double {
        Double(os_proc_available_memory()) / 1_048_576
    }

    /// "123.4 MB".
    static var described: String {
        String(format: "%.1f MB", megabytes)
    }
}
