import SwiftUI

/// Till for iPhone and iPad (docs/ios-plan.md §5). The app starts the Rust
/// core and draws what it publishes: onboarding from phase I2, the till
/// from phase I3. The core lives on the app delegate, which hands it the
/// APNs token (phase I5).
@main
struct TillApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var delegate

    init() {
        Manrope.register()
    }

    var body: some Scene {
        WindowGroup {
            RootView(core: delegate.core)
        }
    }
}
