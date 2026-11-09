import SwiftUI
import TillCore

/// Setting up the till (the Onb*, PadOnb* and PhonePos boards), drawn from
/// the engine's `OnboardingView`: every word that state chooses is the
/// core's, and the headings and leads written once are here, as on Android
/// (docs/decisions/core.md 26). On iPhone each step is a page of its own; on
/// iPad, where the view carries a rail, the step rail stands beside the page.
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
        let label = onboarding.stepLabel
        switch onboarding.page {
        case .welcome(let feature):
            WelcomePage(feature: feature, stepLabel: label, send: send)
        case .scan(let problem):
            ScanPage(problem: problem, stepLabel: label, send: send)
        case .confirm(let host, let encrypted, let facts, let connect, let cancel, let problem):
            ConfirmPage(
                host: host, encrypted: encrypted, facts: facts, connect: connect, cancel: cancel,
                problem: problem, stepLabel: label, send: send
            )
        case .connected(let title, let host, let encrypted, let problem, let facts, let next, let retry, let footnote):
            ConnectedPage(
                title: title, host: host, encrypted: encrypted, problem: problem, facts: facts,
                next: next, retry: retry, footnote: footnote, stepLabel: label, send: send
            )
        case .pointOfSale(let view):
            PointOfSalePage(step: view, stepLabel: label, send: send)
        case .locations(let lead, let locations, let none):
            LocationsPage(lead: lead, locations: locations, none: none, stepLabel: label, send: send)
        case .alerts(let sale, let paid, let next, let notNow):
            AlertsPage(sale: sale, paid: paid, next: next, notNow: notNow, stepLabel: label, send: send)
        case .float(let heading, let lead, _, let next, let notificationsOnly):
            FloatPage(heading: heading, lead: lead, next: next, notificationsOnly: notificationsOnly, send: send)
        case .ready(let heading, let lead, let checks):
            ReadyPage(heading: heading, lead: lead, checks: checks, send: send)
        // C4 and O4 (docs/decisions/core.md 113, 120): their heading, lead
        // and Continue until their own pages are drawn.
        case .counter(let view):
            FloatPage(heading: view.heading, lead: view.lead, next: view.next, notificationsOnly: nil, send: send)
        case .showSales(let view):
            FloatPage(heading: view.heading, lead: view.lead, next: view.next, notificationsOnly: nil, send: send)
        }
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
