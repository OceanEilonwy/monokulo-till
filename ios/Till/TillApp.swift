import SwiftUI

/// Till for iPhone and iPad (docs/ios-plan.md §5). Phase I0: the app starts
/// the Rust core and shows what it publishes; the screens arrive from I1 on.
@main
struct TillApp: App {
    @State private var core = Core()

    init() {
        Manrope.register()
    }

    var body: some Scene {
        WindowGroup {
            CoreView(core: core)
        }
    }
}
