import AVFoundation
import SwiftUI
import TillCore
import UIKit

/// How a page hands the cashier's actions to the engine.
typealias Send = @MainActor (Action) -> Void

/// Words a page writes once, as Android's screens do.
enum Words {
    /// What Till can do with a store, said before connecting (till-screens'
    /// `CAN_DO`, which Connected's footnote carries itself).
    static let canDo = "Till can see this store's orders and create payment codes. It can't spend from or change the wallet. Only connect a server you run or trust."
}

/// Step 1 (OnbWelcome, PadOnb1).
struct WelcomePage: View {
    let feature: String
    let stepLabel: String?
    let send: Send

    var body: some View {
        StepPage(stepLabel: stepLabel) {
            VStack(spacing: 0) {
                LoadingMark(size: 148, label: "Monokulo Till")
                Heading(text: "Monokulo Till", size: 34, centred: true)
                    .padding(.top, 26)
                Lead(
                    text: "Take Monero at your till. Till sees each new sale and shows your customer a payment code.",
                    centred: true
                )
                .frame(maxWidth: 320)
                .padding(.top, 18)
            }
            .frame(maxWidth: .infinity)
            .padding(.horizontal, 24)
            .padding(.top, 48)
            VStack(alignment: .leading, spacing: 22) {
                Feature(symbol: "bag", title: "Sees Monero sales", text: feature)
                Feature(
                    symbol: "qrcode", title: "Shows a payment code",
                    text: "Turn the screen round and the customer scans with their wallet."
                )
                Feature(
                    symbol: "checkmark.circle", title: "Confirms the money arrived",
                    text: "Till tells you when the payment lands, and flags any that don't."
                )
            }
            .padding(.horizontal, 24)
            .padding(.top, 40)
        } bottom: {
            Button("Get started") { send(.next) }
                .buttonStyle(.primary)
        }
    }
}

/// One of Welcome's three features.
struct Feature: View {
    let symbol: String
    let title: String
    let text: String

    var body: some View {
        HStack(alignment: .top, spacing: 16) {
            Image(systemName: symbol)
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
    let problem: String?
    let stepLabel: String?
    let send: Send

    @State private var camera = AVCaptureDevice.authorizationStatus(for: .video)
    @Environment(\.openURL) private var openURL

    var body: some View {
        StepPage(back: { send(.back) }, stepLabel: stepLabel) {
            PageTop(heading: "Connect a store", lead: "Till connects from your Monokulo dashboard, wherever you host it.")
            NumberedCard(steps: [
                "In your Monokulo dashboard, open the store's **Settings › Connections**",
                "Under **Apps**, choose **Connect an app**, then **Show the code**",
                "Point this camera at the code it shows",
            ])
            .padding(.top, 18)
            viewfinder
                .padding(.horizontal, 16)
                .padding(.top, 18)
            ProblemNote(text: problem)
        } bottom: {
            Rich.text("Dashboard open on \(here)? Tap **Copy the connect link** there, then paste it here.")
                .font(.subheadline)
                .foregroundStyle(Palette.secondary)
                .multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
            Button {
                paste()
            } label: {
                Label("Paste the link", systemImage: "doc.on.clipboard")
                    .frame(maxWidth: .infinity)
            }
            .glassButton()
        }
        .task { await askForCamera() }
    }

    /// "this iPhone" or "this iPad".
    private var here: String {
        UIDevice.current.userInterfaceIdiom == .pad ? "this iPad" : "this iPhone"
    }

    private var viewfinder: some View {
        ZStack {
            Color(hex: 0x2B2926)
            if camera == .authorized {
                QRScanner { text in send(.connectLink(url: text)) }
                RoundedRectangle(cornerRadius: 16)
                    .stroke(Color.white, lineWidth: 4)
                    .frame(width: 200, height: 200)
                    .offset(y: -26)
                    .accessibilityHidden(true)
                VStack {
                    Spacer()
                    HStack(spacing: 8) {
                        LoadingMark(size: 22, label: "Scanning", ring: Color.white)
                        Text("Looking for a connect code…")
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(Color.white)
                    }
                    .frame(maxWidth: .infinity, minHeight: 40)
                    .background(Capsule().fill(Color.black.opacity(0.55)))
                    .padding(12)
                }
            } else {
                VStack(spacing: 12) {
                    Text("Allow the camera to scan the code, or paste the link below.")
                        .font(.subheadline)
                        .foregroundStyle(Color.white)
                        .multilineTextAlignment(.center)
                    Button("Allow the camera") { allowCamera() }
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
        let text = UIPasteboard.general.string?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        send(.connectLink(url: text))
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
    let stepLabel: String?
    let send: Send

    var body: some View {
        StepPage(stepLabel: stepLabel) {
            SheetTop(title: "Connect to this store?", host: host, encrypted: encrypted)
            SectionLabel(text: "This device will take payments for")
            FactCard(facts: facts)
            Footnote(text: Words.canDo)
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
    let stepLabel: String?
    let send: Send

    var body: some View {
        StepPage(stepLabel: stepLabel) {
            SheetTop(title: title, host: host, encrypted: encrypted)
            ProblemNote(text: problem)
            SectionLabel(text: "This device takes payments for")
            FactCard(facts: facts)
            Footnote(text: footnote)
        } bottom: {
            PrimaryPress(press: next) { send(retry ? .readStore : .next) }
            LinkButton(label: "Wrong store: disconnect") { send(.disconnect) }
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

/// Step 4 (OnbSquare, PhonePos, PadOnb4): Square or Clover, then the chosen
/// one's card. Also Settings' "Change the point of sale", with Back.
struct PointOfSalePage: View {
    let step: PosStep
    let stepLabel: String?
    let send: Send

    /// Signed in to the chosen point of sale: the buttons go on, not to the sign-in.
    private var signedIn: Bool {
        guard case .some(.connected) = step.card?.account else { return false }
        return true
    }

    private var back: (@MainActor () -> Void)? {
        guard step.back else { return nil }
        return { send(.back) }
    }

    var body: some View {
        StepPage(back: back, stepLabel: stepLabel) {
            PageTop(heading: "Connect your point of sale")
            SectionLabel(text: "Point of sale")
            Card {
                ForEach(Array(step.choices.enumerated()), id: \.offset) { index, choice in
                    if index > 0 {
                        Hairline()
                    }
                    PosChoiceRow(choice: choice) { send(.choosePointOfSale(pos: pos(choice.mark))) }
                }
            }
            if let card = step.card {
                PosCardSection(card: card, send: send)
            }
        } bottom: {
            if let next = step.next {
                PrimaryPress(press: next) { send(signedIn ? .next : .signIn(applicationId: nil)) }
            }
            if let skip = step.skip {
                LinkPress(press: skip) { send(signedIn ? .next : .withoutPos) }
            }
        }
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

/// The chosen point of sale's card: its sign-in, its location or merchant,
/// and what to do at the point of sale itself, with the test sale.
struct PosCardSection: View {
    let card: PosCard
    let send: Send

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 10) {
                Logo(size: 36)
                Image(systemName: "link")
                    .foregroundStyle(Palette.secondary)
                PosMarkTile(mark: card.mark)
            }
            .accessibilityHidden(true)
            Heading(text: card.heading, size: 24)
            Lead(text: card.lead)
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
                        Text("Connected")
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
        SectionLabel(text: card.atTheTill)
        Card {
            VStack(alignment: .leading, spacing: 12) {
                Text(card.stepsTitle)
                    .font(.headline)
                if let first = card.steps.first {
                    numbered(1, first)
                }
                if let lookAgain = card.lookAgain {
                    Button {
                        send(.lookAgain)
                    } label: {
                        Text(lookAgain.label)
                            .frame(maxWidth: .infinity)
                    }
                    .glassButton()
                    .disabled(!lookAgain.enabled)
                }
                ForEach(Array(card.steps.dropFirst().enumerated()), id: \.offset) { index, step in
                    numbered(index + 2, step)
                }
                if let test = card.testSale {
                    HStack(spacing: 10) {
                        if test.found {
                            DoneTick(size: 26)
                        } else {
                            LoadingMark(size: 26, label: "Waiting for a test sale")
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
    let lead: String
    let locations: [LocationRow]
    let noLocations: Fact?
    let stepLabel: String?
    let send: Send

    init(lead: String, locations: [LocationRow], none: Fact?, stepLabel: String?, send: @escaping Send) {
        self.lead = lead
        self.locations = locations
        self.noLocations = none
        self.stepLabel = stepLabel
        self.send = send
    }

    var body: some View {
        StepPage(back: { send(.back) }, stepLabel: stepLabel) {
            PageTop(heading: "Which location is this till at?", lead: lead)
            SectionLabel(text: "Locations")
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
    let sale: SampleAlert
    let paid: SampleAlert
    let next: Press
    let notNow: Press?
    let stepLabel: String?
    let send: Send

    @Environment(\.openURL) private var openURL

    var body: some View {
        StepPage(back: { send(.back) }, stepLabel: stepLabel) {
            PageTop(
                heading: "Hear every new sale",
                lead: "When Till is in the background, an alert tells you a customer is ready to pay."
            )
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
                LinkPress(press: notNow) { send(.next) }
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
            Logo(size: 28)
                .frame(width: 38, height: 38)
                .background(RoundedRectangle(cornerRadius: 9).fill(Palette.ground))
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
        StepPage(back: { send(.back) }, stepLabel: nil) {
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

/// Step 6 (OnbReady, PadOnb6): what's set up, and start the shift.
struct ReadyPage: View {
    let heading: String
    let lead: String
    let checks: [Check]
    let send: Send

    var body: some View {
        StepPage(stepLabel: nil) {
            VStack(spacing: 10) {
                Logo(size: 72)
                    .padding(.bottom, 10)
                Heading(text: heading, centred: true)
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
                    HStack(spacing: 12) {
                        Image(systemName: "checkmark.circle.fill")
                            .foregroundStyle(check.done ? Palette.paid : Palette.border)
                            .accessibilityHidden(true)
                        Text(check.name)
                        Spacer(minLength: 12)
                        Text(check.value)
                            .foregroundStyle(Palette.secondary)
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 12)
                    .frame(minHeight: 48)
                    .accessibilityElement(children: .combine)
                }
            }
            .padding(.top, 40)
        } bottom: {
            Button("Start Till") { send(.startShift) }
                .buttonStyle(.primary)
        }
    }
}
