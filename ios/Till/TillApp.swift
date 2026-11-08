import SwiftUI

/// Till for iPhone and iPad (docs/ios-plan.md §5). The app starts the Rust
/// core and draws what it publishes: onboarding from phase I2, phase I0's
/// core page in place of the till until I3.
@main
struct TillApp: App {
    @State private var core = Core()

    init() {
        Manrope.register()
    }

    var body: some Scene {
        WindowGroup {
            RootView(core: core)
        }
    }
}
