import AVFoundation
import SwiftUI
import TillCore
import UIKit

/// How a page hands the cashier's actions to the engine.
typealias Send = @MainActor (Action) -> Void

/// Step 1 (OnbWelcome, PadOnb1): the engine's heading, lead, features
/// and "Get started" (core.md decision 130).
struct WelcomePage: View {
    let view: OnboardingView
    let send: Send

    var body: some View {
        StepPage {
            VStack(spacing: 0) {
                TillWidget(state: .idle, size: 148, decorative: true)
                Heading(text: view.heading, centred: true, display: 34)
                    .padding(.top, 26)
                Lead(text: view.lead, centred: true)
                    .frame(maxWidth: 320)
                    .padding(.top, 18)
            }
            .frame(maxWidth: .infinity)
            .padding(.horizontal, 24)
            .padding(.top, 48)
            VStack(alignment: .leading, spacing: 22) {
                ForEach(Array(view.features.enumerated()), id: \.offset) { _, feature in
                    FeatureRow(feature: feature)
                }
            }
            .padding(.horizontal, 24)
            .padding(.top, 40)
        } bottom: {
            if let start = view.start {
                PrimaryPress(press: start) { send(.next) }
            }
        }
    }
}

/// Each of Welcome's features' symbol.
enum FeatureLook {
    static func symbol(_ mark: FeatureMark) -> String {
        switch mark {
        case .sales: "bag"
        case .code: "qrcode"
        case .confirmed: "checkmark.circle"
        }
    }
}

/// One of Welcome's three features.
struct FeatureRow: View {
    let feature: Feature

    private var title: String { feature.title }
    private var text: String { feature.text }

    var body: some View {
        HStack(alignment: .top, spacing: 16) {
            Image(systemName: FeatureLook.symbol(feature.mark))
                .font(.title2)
                .foregroundStyle(Palette.link)
                .frame(width: 30)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.headline)
                Text(text)
                    .font(.subheadline)
                    .foregroundStyle(Palette.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .accessibilityElement(children: .combine)
    }
}

/// Numbered steps on a card (OnbStore), `**bold**` for what to tap.
struct NumberedCard: View {
    let steps: [String]

    var body: some View {
        Card {
            ForEach(Array(steps.enumerated()), id: \.offset) { index, step in
                HStack(alignment: .firstTextBaseline, spacing: 12) {
                    Text(String(index + 1))
                        .font(.footnote.weight(.bold))
                        .foregroundStyle(Palette.ground)
                        .frame(width: 22, height: 22)
                        .background(Circle().fill(Palette.ink))
                    Rich.text(step)
                        .font(.subheadline)
                        .fixedSize(horizontal: false, vertical: true)
                    Spacer(minLength: 0)
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 9)
            }
        }
    }
}

/// Step 2 (OnbStore, PadOnb2): the camera looks for the store's connect
/// code, or the link is pasted. Either goes to the engine, which says why
/// one wasn't taken.
struct ScanPage: View {
    let view: OnboardingView
    let problem: String?
    let send: Send

    @State private var camera = AVCaptureDevice.authorizationStatus(for: .video)
    @Environment(\.openURL) private var openURL

    var body: some View {
        StepPage(back: { send(.back) }) {
            PageTop(heading: view.heading, lead: view.lead)
            NumberedCard(steps: view.scan?.steps ?? [])
                .padding(.top, 18)
            viewfinder
                .padding(.horizontal, 16)
                .padding(.top, 18)
            ProblemNote(text: problem)
            if let hint = view.scan?.pasteHint {
                Rich.text(hint)
                    .font(.subheadline)
                    .foregroundStyle(Palette.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 20)
                    .padding(.top, 18)
            }
        } bottom: {
            Button {
                paste()
            } label: {
                Label(view.scan?.paste ?? "", systemImage: "doc.on.clipboard")
            }
            .buttonStyle(.secondary)
        }
        .task { await askForCamera() }
    }

    private var viewfinder: some View {
        ZStack {
            Color(hex: 0x2B2926)
            if camera == .authorized {
                QRScanner { text in send(.scanConnectCode(text: text)) }
                RoundedRectangle(cornerRadius: 16)
                    .stroke(Color.white, lineWidth: 4)
                    .frame(width: 200, height: 200)
                    .offset(y: -26)
                    .accessibilityHidden(true)
                VStack {
                    Spacer()
                    HStack(spacing: 8) {
                        ProgressView()
                            .tint(Color.white)
                        Text(view.scan?.looking ?? "")
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(Color.white)
                    }
                    .frame(maxWidth: .infinity, minHeight: 40)
                    .background(Capsule().fill(Color.black.opacity(0.55)))
                    .padding(12)
                }
            } else {
                VStack(spacing: 12) {
                    Text(view.scan?.cameraAsk ?? "")
                        .font(.subheadline)
                        .foregroundStyle(Color.white)
                        .multilineTextAlignment(.center)
                    Button(view.scan?.allowCamera ?? "") { allowCamera() }
                        .buttonStyle(.primary)
                        .frame(maxWidth: 240)
                }
                .padding(24)
            }
        }
        .frame(height: 300)
        .clipShape(RoundedRectangle(cornerRadius: 24))
    }

    private func paste() {
        let text = UIPasteboard.general.string?.trimmingCharacters(in: .whitespacesAndNewlines)
        send(.pasteConnectLink(text: text))
    }

    private func askForCamera() async {
        guard camera == .notDetermined else { return }
        _ = await AVCaptureDevice.requestAccess(for: .video)
        camera = AVCaptureDevice.authorizationStatus(for: .video)
    }

    /// Asks once; after a refusal, only Settings can allow it.
    private func allowCamera() {
        if camera == .notDetermined {
            Task { await askForCamera() }
        } else if let settings = URL(string: UIApplication.openSettingsURLString) {
            openURL(settings)
        }
    }
}

/// Step 3, before (OnbConfirm, PadOnb3): the server, before the code is spent.
struct ConfirmPage: View {
    let host: String
    let encrypted: Bool
    let facts: [Fact]
    let connect: Press
    let cancel: Press
    let problem: String?
    let view: OnboardingView
    let send: Send

    var body: some View {
        StepPage {
            SheetTop(title: view.heading, host: host, encrypted: encrypted)
            SectionLabel(text: view.section)
            FactCard(facts: facts)
            Footnote(text: view.footnote)
            ProblemNote(text: problem)
        } bottom: {
            PrimaryPress(press: connect) { send(.connect) }
            LinkPress(press: cancel) { send(.back) }
        }
    }
}

/// Step 3, after: the store, its wallet and this device, to check.
struct ConnectedPage: View {
    let title: String
    let host: String
    let encrypted: Bool
    let problem: String?
    let facts: [Fact]
    let next: Press
    let retry: Bool
    let footnote: String
    let view: OnboardingView
    let send: Send

    var body: some View {
        StepPage {
            SheetTop(title: title, host: host, encrypted: encrypted)
            ProblemNote(text: problem)
            SectionLabel(text: view.section)
            FactCard(facts: facts)
            Footnote(text: footnote)
        } bottom: {
            PrimaryPress(press: next) { send(retry ? .readStore : .next) }
            if let disconnect = view.disconnect {
                LinkButton(label: disconnect.label) { send(.disconnect) }
            }
        }
    }
}

/// The engine's point of sale for a mark.
func pos(_ mark: PosMark) -> Pos {
    switch mark {
    case .square: .square
    case .clover: .clover
    }
}

/// The point of sale the cashier chose, as the views on screen say: step 4's
/// card (or its ticked row), else Settings' point of sale, signed in to or
/// ticked. Nil before a choice.
func chosenPos(step: PosStep?, settings: PosSection?) -> PosMark? {
    if let step {
        return step.card?.mark ?? step.choices.first { $0.choice.selected }?.mark
    }
    guard let settings else { return nil }
    switch settings {
    case .signedIn(let mark, _, _, _, _, _, _, _, _, _, _):
        return mark
    case .choosing(let choices, _, _):
        return choices.first { $0.choice.selected }?.mark
    default:
        return nil
    }
}

/// Where a sign-in to `mark` comes back (its setup's `redirectUri`), for a
/// sign-in page that names none: Clover's once Clover is chosen, else
/// Square's.
func redirectUri(_ mark: PosMark?, square: String, clover: String) -> String {
    mark == .clover ? clover : square
}

/// Step 4 (OnbPos, OnbSquare, OnbTest; docs/ipad-redesign.md, decisions 12
/// and 13), in the engine's three stages: O1 the choice alone, with
/// "Continue with Square" and "Set up later" over its footnote; O2 the
/// chosen one's card and its sign-in, with Back to O1; O3 the test sale,
/// its status pinned in the bottom bar beside "Skip the test" and
/// Continue. From Settings ("Change the point of sale") there's no stage:
/// the choice and the card together, with Back.
struct PointOfSalePage: View {
    let step: PosStep
    /// The page's heading and lead, as the engine words each stage.
    let view: OnboardingView
    let send: Send

    private var back: (@MainActor () -> Void)? {
        guard step.back else { return nil }
        return { send(.back) }
    }

    var body: some View {
        StepPage(back: back) {
            switch step.stage {
            case .some(.choose):
                PageTop(heading: view.heading, lead: view.lead)
                PosChoiceTiles(choices: step.choices, send: send)
                    .padding(.top, 20)
            case .some(.signIn):
                if let card = step.card {
                    PosAccountSection(card: card, heading: view.heading, lead: view.lead, send: send)
                }
            case .some(.testSale):
                if let card = step.card {
                    PosConnectedLine(line: step.connected, changeLocation: card.changeLocation, send: send)
                    PageTop(heading: view.heading, lead: view.lead)
                    PosTillSteps(card: card, testRow: false, send: send)
                    ProblemNote(text: card.problem)
                }
            case .none:
                PageTop(heading: view.heading)
                SectionLabel(text: view.section)
                Card {
                    ForEach(Array(step.choices.enumerated()), id: \.offset) { index, choice in
                        if index > 0 {
                            Hairline()
                        }
                        PosChoiceRow(choice: choice) { send(.choosePointOfSale(pos: pos(choice.mark))) }
                    }
                }
                if let card = step.card {
                    PosAccountSection(card: card, send: send)
                    PosTillSteps(card: card, testRow: true, send: send)
                }
            }
        } bottom: {
            if let status = step.status {
                TestStatusBar(status: status, next: step.next, skip: step.skip, send: send)
            } else {
                if let next = step.next {
                    PrimaryPress(press: next) { send(.next) }
                }
                if let skip = step.skip {
                    LinkPress(press: skip) { send(.skip) }
                }
                if let footnote = step.skipFootnote {
                    Text(footnote)
                        .font(.footnote)
                        .foregroundStyle(Palette.secondary)
                        .multilineTextAlignment(.center)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
    }
}


/// O1's choice: each point of sale as a tile, side by side where they fit.
struct PosChoiceTiles: View {
    let choices: [PosChoice]
    let send: Send

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(alignment: .top, spacing: 16) { tiles }
                .frame(minWidth: 520)
            VStack(spacing: 12) { tiles }
        }
        .padding(.horizontal, 16)
    }

    private var tiles: some View {
        ForEach(Array(choices.enumerated()), id: \.offset) { _, choice in
            Button {
                send(.choosePointOfSale(pos: pos(choice.mark)))
            } label: {
                VStack(alignment: .leading, spacing: 12) {
                    HStack {
                        PosMarkTile(mark: choice.mark, inInk: choice.markInInk, size: 44)
                        Spacer(minLength: 8)
                        if choice.choice.selected {
                            Image(systemName: "checkmark.circle.fill")
                                .font(.title2)
                                .foregroundStyle(Palette.ink)
                                .accessibilityHidden(true)
                        }
                    }
                    Text(choice.choice.label)
                        .font(.title3.weight(.semibold))
                        .lineLimit(1)
                    if let detail = choice.choice.detail {
                        Text(detail)
                            .font(.subheadline)
                            .foregroundStyle(Palette.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                .padding(20)
                .frame(maxWidth: .infinity, alignment: .topLeading)
                .background(RoundedRectangle(cornerRadius: 16).fill(Palette.card))
                .overlay(
                    RoundedRectangle(cornerRadius: 16)
                        .strokeBorder(choice.choice.selected ? Palette.ink : Color.clear, lineWidth: 2)
                )
                .contentShape(RoundedRectangle(cornerRadius: 16))
            }
            .buttonStyle(.plain)
            .foregroundStyle(Palette.ink)
            .disabled(!choice.choice.enabled)
            .accessibilityElement(children: .combine)
            .accessibilityAddTraits(choice.choice.selected ? .isSelected : [])
        }
    }
}

/// O3's line over its heading, the engine's `PosStep.connected`:
/// connected, to whom and where, on one line, and Change when the location
/// can change.
struct PosConnectedLine: View {
    let line: String?
    let changeLocation: Bool
    let send: Send

    var body: some View {
        if let line {
            HStack(spacing: 10) {
                DoneTick(size: 22)
                Text(line)
                    .font(.subheadline.weight(.semibold))
                    .lineLimit(1)
                Spacer(minLength: 8)
                if changeLocation {
                    Button("Change") { send(.changeLocation) }
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(Palette.link)
                        .buttonStyle(.plain)
                        .frame(minHeight: minimumTarget)
                        .accessibilityLabel("Change the location")
                }
            }
            .padding(.horizontal, 20)
            .padding(.top, 12)
        }
    }
}

/// O3's status, pinned in the bottom bar: the glyph, what's happening and
/// what follows, each on one line, beside Skip the test and Continue (side
/// by side where they fit, else stacked).
struct TestStatusBar: View {
    let status: TestStatus
    let next: Press?
    let skip: Press?
    let send: Send

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 16) {
                words
                Spacer(minLength: 8)
                if let skip {
                    LinkPress(press: skip) { send(.skip) }
                        .fixedSize()
                }
                if let next {
                    PrimaryPress(press: next) { send(.next) }
                        .frame(width: 200)
                }
            }
            .frame(minWidth: 560)
            VStack(spacing: 10) {
                words
                    .frame(maxWidth: .infinity, alignment: .leading)
                if let next {
                    PrimaryPress(press: next) { send(.next) }
                }
                if let skip {
                    LinkPress(press: skip) { send(.skip) }
                }
            }
        }
    }

    private var words: some View {
        HStack(spacing: 12) {
            if status.found {
                DoneTick(size: 24)
            } else {
                StatusGlyph(kind: .waiting, size: 24)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(status.title)
                    .font(.body.weight(.semibold))
                    .lineLimit(1)
                Text(status.detail)
                    .font(.footnote)
                    .foregroundStyle(Palette.secondary)
                    .lineLimit(1)
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.updatesFrequently)
    }
}

/// A point of sale as a radio row: its mark, name and line, and a tick.
struct PosChoiceRow: View {
    let choice: PosChoice
    let action: @MainActor () -> Void

    var body: some View {
        Button {
            action()
        } label: {
            HStack(spacing: 12) {
                PosMarkTile(mark: choice.mark, inInk: choice.markInInk)
                VStack(alignment: .leading, spacing: 2) {
                    Text(choice.choice.label)
                        .foregroundStyle(Palette.ink)
                    if let detail = choice.choice.detail {
                        Text(detail)
                            .font(.subheadline)
                            .foregroundStyle(Palette.secondary)
                    }
                }
                Spacer(minLength: 8)
                if choice.choice.selected {
                    Image(systemName: "checkmark")
                        .font(.body.weight(.semibold))
                        .foregroundStyle(Palette.link)
                        .accessibilityHidden(true)
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
            .frame(minHeight: 56)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!choice.choice.enabled)
        .accessibilityAddTraits(choice.choice.selected ? .isSelected : [])
    }
}

/// The chosen point of sale's card (O2): its heading and lead (O2's own,
/// "Sign in to Square", where the engine words the page), the sign-in,
/// its location or merchant, and what Till may do there.
struct PosAccountSection: View {
    let card: PosCard
    var heading: String? = nil
    var lead: String? = nil
    let send: Send

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Heading(text: heading ?? card.heading)
            Lead(text: lead ?? card.lead)
        }
        .padding(.horizontal, 20)
        .padding(.top, 24)
        SectionLabel(text: card.accountTitle)
        Card {
            switch card.account {
            case .notSignedIn(let row):
                FactRow(fact: row)
            case .connected(let who):
                HStack(spacing: 12) {
                    DoneTick()
                    VStack(alignment: .leading, spacing: 2) {
                        Text(card.connected)
                        Text(who)
                            .font(.subheadline)
                            .foregroundStyle(Palette.secondary)
                    }
                    Spacer(minLength: 0)
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 8)
                .frame(minHeight: 52)
                .accessibilityElement(children: .combine)
            }
            if let place = card.place {
                Hairline()
                if card.changeLocation {
                    Button {
                        send(.changeLocation)
                    } label: {
                        FactRow(fact: place)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityHint("Change the location")
                } else {
                    FactRow(fact: place)
                }
            }
        }
        Footnote(text: card.permissions)
        ProblemNote(text: card.problem)
    }
}

/// What to do at the point of sale itself (O3): its steps and, from
/// Settings, the test sale's row; O3 pins the test sale's status in its
/// bottom bar instead.
struct PosTillSteps: View {
    let card: PosCard
    let testRow: Bool
    let send: Send

    var body: some View {
        SectionLabel(text: card.atTheTill)
        Card {
            VStack(alignment: .leading, spacing: 12) {
                Text(card.stepsTitle)
                    .font(.headline)
                if let first = card.steps.first {
                    numbered(1, first)
                }
                if let lookAgain = card.lookAgain {
                    Button(lookAgain.label) { send(.lookAgain) }
                        .buttonStyle(.secondary)
                        .disabled(!lookAgain.enabled)
                }
                ForEach(Array(card.steps.dropFirst().enumerated()), id: \.offset) { index, step in
                    numbered(index + 2, step)
                }
                if testRow, let test = card.testSale {
                    HStack(spacing: 10) {
                        if test.found {
                            DoneTick(size: 26)
                        } else {
                            ProgressView()
                                .frame(width: 26, height: 26)
                                .accessibilityLabel("Waiting for a test sale")
                        }
                        Text(test.text)
                            .font(.subheadline)
                            .fixedSize(horizontal: false, vertical: true)
                        Spacer(minLength: 0)
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 10)
                    .background(RoundedRectangle(cornerRadius: 10).fill(Palette.ground))
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 14)
        }
        if let footnote = card.footnote {
            Footnote(text: footnote)
        }
    }

    private func numbered(_ number: Int, _ step: String) -> some View {
        Rich.text("\(number).  \(step)")
            .font(.subheadline)
            .fixedSize(horizontal: false, vertical: true)
    }
}

/// Step 4, when Square has more than one location.
struct LocationsPage: View {
    let view: OnboardingView
    let locations: [LocationRow]
    let noLocations: Fact?
    let send: Send

    init(view: OnboardingView, locations: [LocationRow], none: Fact?, send: @escaping Send) {
        self.view = view
        self.locations = locations
        self.noLocations = none
        self.send = send
    }

    var body: some View {
        StepPage(back: { send(.back) }) {
            PageTop(heading: view.heading, lead: view.lead)
            SectionLabel(text: view.section)
            if let noLocations {
                FactCard(facts: [noLocations])
            } else {
                Card {
                    ForEach(Array(locations.enumerated()), id: \.offset) { index, location in
                        if index > 0 {
                            Hairline()
                        }
                        Button {
                            send(.chooseLocation(locationId: location.id))
                        } label: {
                            FactRow(fact: Fact(label: location.name, value: location.currency, tone: .plain, mono: false))
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel(location.choose)
                    }
                }
            }
        } bottom: {
            EmptyView()
        }
    }
}

/// Step 5 (OnbAlerts, PadOnb5): alerts, with two as they will look.
struct AlertsPage: View {
    let view: OnboardingView
    let sale: SampleAlert
    let paid: SampleAlert
    let next: Press
    let notNow: Press?
    let send: Send

    @Environment(\.openURL) private var openURL

    var body: some View {
        StepPage(back: { send(.back) }) {
            PageTop(heading: view.heading, lead: view.lead)
            VStack(spacing: 10) {
                SampleAlertCard(alert: sale, faint: false)
                SampleAlertCard(alert: paid, faint: true)
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 26)
            .background(RoundedRectangle(cornerRadius: 28).fill(Palette.hairline))
            .padding(.horizontal, 16)
            .padding(.top, 36)
            .accessibilityHidden(true)
        } bottom: {
            PrimaryPress(press: next) { Task { await turnOn() } }
            if let notNow {
                LinkPress(press: notNow) { send(.skip) }
            }
        }
    }

    /// Asks while iOS still can; after a refusal, alerts are turned on in
    /// Settings, and the engine hears of it when Till comes back.
    private func turnOn() async {
        let status = await AlertsPermission.status()
        switch status {
        case .notDetermined:
            let allowed = await AlertsPermission.ask()
            send(.permissions(alerts: allowed, bubble: false))
            send(.next)
        case .denied:
            if let settings = URL(string: UIApplication.openNotificationSettingsURLString) {
                openURL(settings)
            }
        default:
            send(.permissions(alerts: AlertsPermission.allows(status), bubble: false))
            send(.next)
        }
    }
}

/// An alert as it will look, in step 5's picture.
struct SampleAlertCard: View {
    let alert: SampleAlert
    let faint: Bool

    var body: some View {
        HStack(alignment: alert.text == nil ? .center : .top, spacing: 12) {
            TillWidget(state: .idle, size: 38, decorative: true)
            VStack(alignment: .leading, spacing: 2) {
                if alert.text != nil {
                    HStack {
                        Text("Till")
                            .font(.footnote.weight(.semibold))
                            .foregroundStyle(Palette.secondary)
                        Spacer()
                        Text(alert.time)
                            .font(.footnote)
                            .foregroundStyle(Palette.secondary)
                    }
                }
                Text(alert.title)
                    .font(.subheadline.weight(.semibold))
                if let text = alert.text {
                    Text(text)
                        .font(.subheadline)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            if alert.text == nil {
                Spacer(minLength: 8)
                Text(alert.time)
                    .font(.footnote)
                    .foregroundStyle(Palette.secondary)
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 20).fill(Palette.card.opacity(faint ? 0.6 : 0.92)))
    }
}

/// Step 5 on Android (the bubble), which the engine never shows on iOS;
/// drawn plainly so every page has a face.
struct FloatPage: View {
    let heading: String
    let lead: String
    let next: Press
    let notificationsOnly: Press?
    let send: Send

    var body: some View {
        StepPage(back: { send(.back) }) {
            PageTop(heading: heading, lead: lead)
        } bottom: {
            PrimaryPress(press: next) { send(.next) }
            if let notificationsOnly {
                LinkPress(press: notificationsOnly) {
                    send(.setBubble(on: false))
                    send(.next)
                }
            }
        }
    }
}

/// C4 (CounterLearn; docs/ipad-redesign.md, decision 4): the test sale
/// named this counter's device, and which sales alert here.
struct CounterPage: View {
    let view: CounterStep
    let send: Send

    var body: some View {
        StepPage(back: { send(.back) }) {
            HStack(spacing: 12) {
                DoneTick(size: 22)
                Text(view.found)
                    .font(.subheadline.weight(.semibold))
                    .lineLimit(1)
            }
            .padding(.horizontal, 20)
            .padding(.top, 12)
            .accessibilityElement(children: .combine)
            PageTop(heading: view.heading, lead: view.lead)
            ChoiceCard(choices: view.choices) { index in
                send(.setAlertFor(everyDevice: index > 0))
            }
            .padding(.top, 20)
            Footnote(text: view.footnote)
        } bottom: {
            PrimaryPress(press: view.next) { send(.next) }
        }
    }
}

/// O4 (OnbShow; docs/ipad-redesign.md, decisions 1 to 3 and 14): how a new
/// sale shows up, how side by side is set up while it's chosen, and the
/// sample alert, which "Send a test alert" posts.
struct ShowSalesPage: View {
    let view: ShowSalesStep
    let send: Send

    var body: some View {
        StepPage(back: { send(.back) }) {
            PageTop(heading: view.heading, lead: view.lead)
            Card {
                ForEach(Array(view.choices.enumerated()), id: \.offset) { index, choice in
                    if index > 0 {
                        Hairline()
                    }
                    ShowSalesRow(choice: choice, padded: true) { send(.chooseShowSales(mode: choice.mode)) }
                }
            }
            .padding(.top, 20)
            if let how = view.sideBySide {
                Card {
                    VStack(alignment: .leading, spacing: 6) {
                        Text(how.label)
                            .font(.subheadline.weight(.semibold))
                        Text(how.value)
                            .font(.subheadline)
                            .foregroundStyle(Palette.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    .padding(16)
                }
                .padding(.top, 16)
            }
            SaleAlertPreview(alert: view.sample)
                .padding(.horizontal, 14)
                .padding(.vertical, 22)
                .background(RoundedRectangle(cornerRadius: 28).fill(Palette.hairline))
                .padding(.horizontal, 16)
                .padding(.top, 28)
                .accessibilityHidden(true)
        } bottom: {
            PrimaryPress(press: view.next) { send(.next) }
            LinkPress(press: view.testAlert) {
                let sample = view.sample
                Task { await SaleAlerts.post(sample) }
            }
        }
    }
}

/// One way a new sale can show up, led by its picture (OnbShow's
/// skeletons), ticked when chosen: the same row in O4 and in Settings
/// (core.md decision 182). `padded` on O4's card, where no list pads it.
struct ShowSalesRow: View {
    let choice: ShowSalesChoice
    var padded = false
    let action: @MainActor () -> Void

    var body: some View {
        Button {
            action()
        } label: {
            HStack(spacing: 14) {
                ShowSalesPicture(mode: choice.mode)
                VStack(alignment: .leading, spacing: 2) {
                    Text(choice.choice.label)
                        .foregroundStyle(Palette.ink)
                    if let detail = choice.choice.detail {
                        Text(detail)
                            .font(.subheadline)
                            .foregroundStyle(Palette.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                Spacer(minLength: 8)
                if choice.choice.selected {
                    Image(systemName: "checkmark")
                        .font(.body.weight(.semibold))
                        .foregroundStyle(Palette.link)
                        .accessibilityHidden(true)
                }
            }
            .padding(.horizontal, padded ? 16 : 0)
            .padding(.vertical, padded ? 12 : 4)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!choice.choice.enabled)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(choice.choice.selected ? .isSelected : [])
    }
}

/// OnbShow's skeleton of a way a sale shows up, on a 56-point tile: an
/// alert's pill, a floating window in the corner of the point of sale, or
/// the two side by side. A picture only; the row says it in words.
struct ShowSalesPicture: View {
    let mode: ShowSales

    var body: some View {
        RoundedRectangle(cornerRadius: 12, style: .continuous)
            .fill(Palette.ground)
            .frame(width: 56, height: 56)
            .overlay { skeleton }
            .accessibilityHidden(true)
    }

    @ViewBuilder
    private var skeleton: some View {
        switch mode {
        case .alert:
            Capsule()
                .fill(Palette.ink)
                .frame(width: 36, height: 14)
        case .floatingWindow:
            RoundedRectangle(cornerRadius: 4)
                .fill(Palette.border)
                .frame(width: 40, height: 30)
                .overlay(alignment: .bottomTrailing) {
                    RoundedRectangle(cornerRadius: 3)
                        .fill(Palette.ink)
                        .frame(width: 14, height: 14)
                        .padding(3)
                }
        case .sideBySide:
            HStack(spacing: 3) {
                RoundedRectangle(cornerRadius: 4)
                    .fill(Palette.border)
                    .frame(width: 24, height: 30)
                RoundedRectangle(cornerRadius: 4)
                    .fill(Palette.ink)
                    .frame(width: 12, height: 30)
            }
        }
    }
}

/// Radio choices on a card: each one's label and line, ticked when chosen.
/// `choose` gets the tapped one's place.
struct ChoiceCard: View {
    let choices: [Choice]
    let choose: @MainActor (Int) -> Void

    var body: some View {
        Card {
            ForEach(Array(choices.enumerated()), id: \.offset) { index, choice in
                if index > 0 {
                    Hairline()
                }
                ChoiceRow(choice: choice, padded: true) { choose(index) }
            }
        }
    }
}

/// A sale's alert as iOS will show it, in O4's picture.
struct SaleAlertPreview: View {
    let alert: SaleAlert

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            TillWidget(state: .idle, size: 38, decorative: true)
            VStack(alignment: .leading, spacing: 2) {
                HStack {
                    Text("Till")
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(Palette.secondary)
                    Spacer()
                    Text("now")
                        .font(.footnote)
                        .foregroundStyle(Palette.secondary)
                }
                Text(alert.title)
                    .font(.subheadline.weight(.semibold))
                    .lineLimit(1)
                Text(alert.body)
                    .font(.subheadline)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 20).fill(Palette.card.opacity(0.92)))
    }
}

/// Step 6 (OnbReady, PadOnb6): what's set up, and start the shift.
struct ReadyPage: View {
    let heading: String
    let lead: String
    let checks: [Check]
    /// "Start Till".
    let start: Press?
    let send: Send

    var body: some View {
        StepPage {
            VStack(spacing: 10) {
                TillWidget(state: .idle, size: 96, decorative: true)
                    .padding(.bottom, 10)
                Heading(text: heading, centred: true, display: 30)
                Lead(text: lead, centred: true)
                    .frame(maxWidth: 330)
            }
            .frame(maxWidth: .infinity)
            .padding(.horizontal, 20)
            .padding(.top, 56)
            Card {
                ForEach(Array(checks.enumerated()), id: \.offset) { index, check in
                    if index > 0 {
                        Hairline()
                    }
                    // Done and not done differ by shape as well as colour,
                    // and say so aloud (the review's C1).
                    HStack(spacing: 12) {
                        Image(systemName: ReadyPage.symbol(done: check.done))
                            .foregroundStyle(check.done ? Palette.ink : Palette.expiredInk)
                        Text(check.name)
                        Spacer(minLength: 12)
                        Text(check.value)
                            .foregroundStyle(Palette.secondary)
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 12)
                    .frame(minHeight: 48)
                    .accessibilityElement(children: .combine)
                    .accessibilityValue(check.done ? "Done" : "Not set up")
                }
            }
            .padding(.top, 40)
        } bottom: {
            if let start {
                PrimaryPress(press: start) { send(.startShift) }
            }
        }
    }

    /// A check's glyph: a tick done, a dashed circle not.
    static func symbol(done: Bool) -> String {
        done ? "checkmark.circle.fill" : "circle.dashed"
    }
}
