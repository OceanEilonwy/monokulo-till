import SwiftUI

/// The app's one window: the screen the engine says, links that open the
/// app handed to it, and the app's coming and going. When a move between
/// the till's screens settles, the engine hears where (`Action.landed`,
/// decision 55): a new sale rises out of the shift only while it rests.
struct RootView: View {
    let core: Core

    @Environment(\.scenePhase) private var scenePhase

    var body: some View {
        content
            // What the engine is busy with and what went wrong, where no
            // screen's own view says it (docs/decisions/core.md 109).
            .overlay(alignment: .top) {
                VStack(spacing: 0) {
                    if core.shownBusy {
                        ProgressView()
                            .padding(.top, 8)
                            .tint(Palette.secondary)
                    }
                    ProblemNote(text: core.shownProblem)
                        .background(core.shownProblem == nil ? Color.clear : Palette.ground)
                }
            }
            // Universal links on the build's link host and the monokulo-till scheme.
            .onOpenURL { url in core.open(url) }
            .onChange(of: scenePhase) { _, phase in
                switch phase {
                case .active: core.opened()
                case .background: core.backgrounded()
                default: break
                }
            }
            .task { await core.readPermissions() }
            .task(id: core.screen) { [screen = core.screen] in
                await core.land(screen)
            }
    }

    @ViewBuilder
    private var content: some View {
        switch core.route {
        case .starting:
            StartingView()
        case .onboarding:
            if let onboarding = core.onboarding {
                OnboardingScreen(core: core, onboarding: onboarding)
            } else {
                StartingView()
            }
        default:
            TillScreen(core: core, route: core.route)
        }
    }
}

/// While the engine reads what it kept: the system's spinner on the ground.
struct StartingView: View {
    var body: some View {
        ProgressView()
            .accessibilityLabel("Starting")
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Palette.ground)
    }
}
