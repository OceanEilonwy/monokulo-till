import SwiftUI
import TillCore

/// Setting up the till (the Onb*, PadOnb* and PhonePos boards), drawn from
/// the engine's `OnboardingView`: every word is the core's, the headings
/// and leads written once too (docs/decisions/core.md 130, which ends 26's
/// split on iOS). On iPhone each step is a page of its own,
/// under a thin bar of how far setup has come; on iPad, where the view
/// carries a rail, the step rail stands beside the page.
/// It sits in a navigation stack so Back is the system's toolbar button and
/// nothing of Till's goes under iPadOS's window controls; each page scrolls
/// across its whole width, its content held to a readable column.
struct OnboardingScreen: View {
    let core: Core
    let onboarding: OnboardingView

    var body: some View {
        NavigationStack {
            Group {
                if onboarding.rail.isEmpty {
                    page
                } else {
                    HStack(spacing: 0) {
                        StepRail(steps: onboarding.rail)
                            .frame(width: 300)
                        Rectangle()
                            .fill(Palette.hairline)
                            .frame(width: 1)
                            .ignoresSafeArea()
                        page
                    }
                }
            }
            .background(Palette.ground)
            .navigationBarTitleDisplayMode(.inline)
        }
    }

    /// The cashier's actions, to the engine.
    private var send: Send {
        { action in core.send(action) }
    }

    @ViewBuilder
    private var page: some View {
        steps
            .safeAreaInset(edge: .top, spacing: 0) {
                if let progress = onboarding.progress {
                    SetupProgress(progress: progress)
                }
            }
    }

    /// The step's page.
    @ViewBuilder
    private var steps: some View {
        switch onboarding.page {
        case .welcome:
            WelcomePage(view: onboarding, send: send)
        case .scan(let problem):
            ScanPage(view: onboarding, problem: problem, send: send)
        case .confirm(let host, let encrypted, let facts, let connect, let cancel, let problem):
            ConfirmPage(
                host: host, encrypted: encrypted, facts: facts, connect: connect, cancel: cancel,
                problem: problem, view: onboarding, send: send
            )
        case .connected(let title, let host, let encrypted, let problem, let facts, let next, let retry, let footnote):
            ConnectedPage(
                title: title, host: host, encrypted: encrypted, problem: problem, facts: facts,
                next: next, retry: retry, footnote: footnote, view: onboarding, send: send
            )
        case .pointOfSale(let view):
            PointOfSalePage(step: view, view: onboarding, send: send)
        case .locations(_, let locations, let none):
            LocationsPage(view: onboarding, locations: locations, none: none, send: send)
        case .counter(let view):
            CounterPage(view: view, send: send)
        case .alerts(let sale, let paid, let next, let notNow):
            AlertsPage(view: onboarding, sale: sale, paid: paid, next: next, notNow: notNow, send: send)
        case .float(let heading, let lead, _, let next, let notificationsOnly):
            FloatPage(heading: heading, lead: lead, next: next, notificationsOnly: notificationsOnly, send: send)
        case .showSales(let view):
            ShowSalesPage(view: view, send: send)
        case .ready(let heading, let lead, let checks):
            ReadyPage(heading: heading, lead: lead, checks: checks, start: onboarding.start, send: send)
        case .paymentsMode(let view):
            PaymentsModePage(step: view, view: onboarding, send: send)
        case .walletEntry(let view):
            WalletEntryPage(step: view, view: onboarding, send: send)
        case .fobRequest(let view):
            FobRequestPage(step: view, view: onboarding, send: send)
        case .fobConnected(let view):
            FobConnectedPage(step: view, view: onboarding, send: send)
        case .walletCode(let view):
            WalletCodePage(step: view, view: onboarding, send: send)
        case .walletKeys(let view):
            WalletKeysPage(step: view, view: onboarding, send: send)
        case .walletSeed(let view):
            WalletSeedPage(step: view, view: onboarding, send: send)
        case .thisTill(let view):
            ThisTillPage(step: view, view: onboarding, send: send)
        case .prices(let view):
            PricesPage(step: view, view: onboarding, send: send)
        }
    }
}

/// iPhone's thin bar of how far setup has come (decision 11), under the
/// toolbar; none on Welcome. It says no "Step N of M".
struct SetupProgress: View {
    let progress: StepProgress

    var body: some View {
        ProgressView(value: SetupProgress.fraction(progress))
            .tint(Palette.progress)
            .padding(.horizontal, 20)
            .padding(.vertical, 6)
            .background(Palette.ground)
            .accessibilityLabel("Setting up")
    }

    nonisolated static func fraction(_ progress: StepProgress) -> Double {
        guard progress.total > 0 else { return 0 }
        return min(1, Double(progress.value) / Double(progress.total))
    }
}

/// iPad's step rail (PadOnbRail): every step, the current one lit, those
/// done ticked.
struct StepRail: View {
    let steps: [RailStep]

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Monokulo Till")
                .font(.headline)
                .padding(.bottom, 24)
            ForEach(steps, id: \.number) { step in
                HStack(spacing: 14) {
                    marker(step)
                    Text(step.label)
                        .font(step.state == .current ? .body.weight(.semibold) : .body)
                        .foregroundStyle(step.state == .todo ? Palette.secondary : Palette.ink)
                    Spacer(minLength: 0)
                }
                .frame(minHeight: 52)
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(spoken(step))
                .accessibilityAddTraits(step.state == .current ? .isSelected : [])
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 28)
        .padding(.top, 16)
        .frame(maxHeight: .infinity, alignment: .top)
        .foregroundStyle(Palette.ink)
        .background(Palette.card.opacity(0.5))
    }

    @ViewBuilder
    private func marker(_ step: RailStep) -> some View {
        switch step.state {
        case .done:
            DoneTick(size: 28)
                .accessibilityHidden(true)
        case .current:
            Text(String(step.number))
                .font(.subheadline.weight(.bold))
                .foregroundStyle(Palette.onAccent)
                .frame(width: 28, height: 28)
                .background(Circle().fill(Palette.accent))
        case .todo:
            Text(String(step.number))
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(Palette.secondary)
                .frame(width: 28, height: 28)
                .overlay(Circle().stroke(Palette.track, lineWidth: 1.5))
        }
    }

    private func spoken(_ step: RailStep) -> String {
        switch step.state {
        case .done: "Step \(step.number), \(step.label), done"
        case .current: "Step \(step.number), \(step.label), current"
        case .todo: "Step \(step.number), \(step.label)"
        }
    }
}
