import SwiftUI
import TillCore
import UIKit

/// Settings (PadSettings, in the Slide Over panel; PhoneSettings), drawn
/// from the engine's `SettingsView` as an inset grouped list: the store,
/// the point of sale, alerts, Wake-ups (its own page, pushed here, as
/// `Screen` has none), the Dynamic Island on iPhone, the debug builds'
/// stand-ins, and Disconnect. Section titles and Disconnect's footnote are
/// written once here (decision 26).
struct SettingsScreen: View {
    let view: SettingsView
    let wakeUps: WakeUpsView?
    let send: Send

    @Environment(\.openURL) private var openURL
    @State private var askDisconnect = false

    var body: some View {
        NavigationStack {
            List {
                if let store = view.store {
                    StoreSettings(store: store)
                }
                PosSettings(section: view.pointOfSale, send: send)
                Section {
                    // iOS keeps the switch: it opens Till's notification settings.
                    SwitchRow(label: "New sale alerts", on: view.alerts.on, snapsBack: true) { _ in openAlertSettings() }
                } header: {
                    Text("Alerts")
                } footer: {
                    Text(view.alerts.footnote)
                }
                wakeUpsSection
                if let island = view.island {
                    Section("Dynamic Island") {
                        SwitchRow(label: "Live Activity", on: island.liveActivity) { send(.setLiveActivity(on: $0)) }
                        SwitchRow(label: "Show code in island", on: island.codeInIsland) { send(.setCodeInIsland(on: $0)) }
                    }
                }
                if !view.standIns.isEmpty {
                    Section("Developer (debug builds)") {
                        ForEach(Array(view.standIns.enumerated()), id: \.offset) { _, standIn in
                            StandInField(standIn: standIn) { url in
                                send(.setBaseUrl(pos: pos(standIn.pointOfSale), url: url))
                            }
                        }
                    }
                }
                Section {
                    Button("Disconnect this device", role: .destructive) { askDisconnect = true }
                        .foregroundStyle(Palette.danger)
                } footer: {
                    Text("Removes Till from the store's Apps: its key stops working at once. Payments already made aren't affected.")
                }
            }
            .listStyle(.insetGrouped)
            .scrollContentBackground(.hidden)
            .background(Palette.ground)
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button {
                        send(.back)
                    } label: {
                        Image(systemName: "chevron.backward")
                    }
                    .accessibilityLabel("Back")
                }
            }
            .confirmationDialog("Disconnect this device?", isPresented: $askDisconnect, titleVisibility: .visible) {
                Button("Disconnect", role: .destructive) { send(.disconnect) }
            }
        }
        .tint(Palette.link)
    }

    @ViewBuilder
    private var wakeUpsSection: some View {
        switch view.wakeUps {
        case .row(let row):
            Section("Wake-ups") {
                NavigationLink {
                    WakeUpsScreen(view: wakeUps, send: send)
                } label: {
                    LabeledContent(row.label, value: row.value)
                }
            }
        case .inline(let inline):
            WakeUpsSections(view: inline, send: send)
        default:
            EmptyView()
        }
    }

    private func openAlertSettings() {
        if let settings = URL(string: UIApplication.openNotificationSettingsURLString) {
            openURL(settings)
        }
    }
}

/// The store, its wallet and this device.
struct StoreSettings: View {
    let store: StoreSection

    var body: some View {
        Section {
            if let problem = store.problem {
                Text(problem)
                    .font(.subheadline)
                    .foregroundStyle(Palette.danger)
            }
            VStack(alignment: .leading, spacing: 6) {
                Text(store.name)
                    .font(.headline)
                HStack(spacing: 10) {
                    HostChip(host: store.host, encrypted: store.encrypted)
                    HStack(spacing: 6) {
                        Circle()
                            .fill(store.online ? Palette.paid : Palette.secondary)
                            .frame(width: 8, height: 8)
                        Text(store.onlineLabel)
                            .font(.footnote.weight(.semibold))
                            .foregroundStyle(store.online ? Palette.paid : Palette.secondary)
                    }
                    .accessibilityElement(children: .combine)
                }
            }
            .padding(.vertical, 4)
            ForEach(Array(store.facts.enumerated()), id: \.offset) { _, fact in
                if fact.mono, let address = store.walletAddress {
                    HStack(alignment: .firstTextBaseline) {
                        LabeledContent(fact.label) {
                            Text(fact.value)
                                .font(.body.monospaced())
                                .foregroundStyle(toneColor(fact.tone))
                        }
                        Button {
                            UIPasteboard.general.string = address
                        } label: {
                            Image(systemName: "doc.on.doc")
                        }
                        .buttonStyle(.borderless)
                        .accessibilityLabel("Copy the wallet address")
                    }
                } else {
                    FactLine(fact: fact)
                }
            }
        } header: {
            Text("Store")
        }
    }
}

/// A fact as a settings row: its label, and its value in its tone.
struct FactLine: View {
    let fact: Fact

    var body: some View {
        LabeledContent(fact.label) {
            Text(fact.value)
                .font(fact.mono ? .body.monospaced() : .body)
                .foregroundStyle(toneColor(fact.tone))
                .multilineTextAlignment(.trailing)
        }
    }
}

/// Settings › Point of sale: signed in, which one and its facts, Sign out
/// and Change the point of sale; not signed in, the choices and the chosen
/// one's sign-in.
struct PosSettings: View {
    let section: PosSection
    let send: Send

    var body: some View {
        switch section {
        case .signedIn(let mark, let markInInk, let name, let account, let place, let changeLocation, let paymentType, let lookAgain, let signOut, let change, let footnotes):
            Section {
                HStack(spacing: 12) {
                    PosMarkTile(mark: mark, inInk: markInInk)
                    Text(name)
                        .font(.headline)
                }
                if let account {
                    FactLine(fact: account)
                }
                if changeLocation {
                    Button {
                        send(.changeLocation)
                    } label: {
                        FactLine(fact: place)
                    }
                    .foregroundStyle(Palette.ink)
                } else {
                    FactLine(fact: place)
                }
                FactLine(fact: paymentType)
                if let lookAgain {
                    Button(lookAgain) { send(.lookAgain) }
                }
                Button(change) { send(.openPointOfSale) }
                Button(signOut, role: .destructive) { send(.signOutOfPos) }
                    .foregroundStyle(Palette.danger)
            } header: {
                Text("Point of sale")
            } footer: {
                if !footnotes.isEmpty {
                    Text(footnotes.joined(separator: "\n\n"))
                }
            }
        case .choosing(let choices, let signIn, let footnote):
            Section {
                ForEach(Array(choices.enumerated()), id: \.offset) { _, choice in
                    PosChoiceRow(choice: choice) { send(.choosePointOfSale(pos: pos(choice.mark))) }
                        .listRowInsets(EdgeInsets())
                }
                if let signIn {
                    Button(signIn) { send(.signIn(applicationId: nil)) }
                }
            } header: {
                Text("Point of sale")
            } footer: {
                Text(footnote)
            }
        default:
            EmptyView()
        }
    }
}

/// A debug build's stand-in address for a point of sale.
struct StandInField: View {
    let standIn: StandIn
    let use: @MainActor (String) -> Void

    @State private var text = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(standIn.label)
                .font(.footnote)
                .foregroundStyle(Palette.secondary)
            TextField(standIn.label, text: $text)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .keyboardType(.URL)
                .onSubmit { use(text) }
        }
        .onChange(of: standIn.value, initial: true) { _, value in text = value }
    }
}

/// Settings › Wake-ups (PhoneWakeUps; on iPad, in the panel), drawn from
/// the engine's `WakeUpsView`.
struct WakeUpsScreen: View {
    let view: WakeUpsView?
    let send: Send

    var body: some View {
        List {
            if let view {
                WakeUpsSections(view: view, send: send)
            }
        }
        .listStyle(.insetGrouped)
        .scrollContentBackground(.hidden)
        .background(Palette.ground)
        .navigationTitle("Wake-ups")
        .navigationBarTitleDisplayMode(.inline)
    }
}

/// Wake-ups' sections: the three ways sales come, My own Bell's address,
/// the notes, the chosen Bell's status, and on iOS whether alerts allow it.
struct WakeUpsSections: View {
    let view: WakeUpsView
    let send: Send

    @Environment(\.openURL) private var openURL
    /// The field as typed here, sent to the engine as it changes
    /// (`Action.editOwnBell`), which checks it and keeps it.
    @State private var typed = ""

    var body: some View {
        if let problem = view.problem {
            Section {
                Text(problem)
                    .font(.subheadline)
                    .foregroundStyle(Palette.danger)
            }
        }
        Section {
            ChoiceRow(choice: view.timer) { send(.chooseWakeUps(choice: .timer)) }
            ChoiceRow(choice: view.monokuloBell) { send(.chooseWakeUps(choice: .monokuloBell)) }
            // With an address saved the engine picks it at once; with none, its
            // field opens and the saved choice stays ticked until "Use this Bell".
            ChoiceRow(choice: view.ownBell) { send(.editOwnBell(picked: true, typed: nil)) }
            if let field = view.ownField {
                ownField(field)
            }
        } footer: {
            if !view.notes.isEmpty {
                Text(view.notes.joined(separator: "\n\n"))
            }
        }
        if let status = view.status {
            Section {
                FactLine(fact: status.checking)
                FactLine(fact: status.bell)
                if let last = status.lastWakeUp {
                    FactLine(fact: last)
                }
                statusAction(status.action)
            } footer: {
                Text(status.footnote + "\n\n" + status.privacy)
            }
        }
        if let alerts = view.alerts {
            Section {
                FactLine(fact: alerts.alerts)
            } footer: {
                Text(alerts.footnote)
            }
        }
    }

    private func ownField(_ field: OwnBellField) -> some View {
        let address = typed.trimmingCharacters(in: .whitespaces)
        return VStack(alignment: .leading, spacing: 8) {
            TextField("https://", text: $typed)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .keyboardType(.URL)
                .onChange(of: typed) { _, now in
                    guard now != field.initial || view.ownBell.selected == false else { return }
                    send(.editOwnBell(picked: !view.ownBell.selected, typed: now))
                }
            if let problem = field.problem {
                Text(problem)
                    .font(.footnote)
                    .foregroundStyle(Palette.danger)
            }
            Button(field.useThis.label) {
                send(.chooseWakeUps(choice: .ownBell(address: address)))
            }
            .disabled(!field.useThis.enabled || address.isEmpty)
        }
        .padding(.vertical, 4)
        .onAppear {
            if typed.isEmpty { typed = field.initial }
        }
    }

    @ViewBuilder
    private func statusAction(_ action: StatusAction?) -> some View {
        switch action {
        case .some(.openAlertSettings):
            Button("Open alert settings") {
                if let settings = URL(string: UIApplication.openNotificationSettingsURLString) {
                    openURL(settings)
                }
            }
        case .some(.signIn(let label, _, let applicationId)):
            Button(label) { send(.signIn(applicationId: applicationId)) }
        default:
            EmptyView()
        }
    }
}

/// A radio row: its label and line, ticked when chosen.
struct ChoiceRow: View {
    let choice: Choice
    let action: @MainActor () -> Void

    var body: some View {
        Button {
            action()
        } label: {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(choice.label)
                        .foregroundStyle(Palette.ink)
                    if let detail = choice.detail {
                        Text(detail)
                            .font(.subheadline)
                            .foregroundStyle(Palette.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                Spacer(minLength: 8)
                if choice.selected {
                    Image(systemName: "checkmark")
                        .font(.body.weight(.semibold))
                        .foregroundStyle(Palette.link)
                        .accessibilityHidden(true)
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!choice.enabled)
        .accessibilityAddTraits(choice.selected ? .isSelected : [])
    }
}

/// A switch row whose state is the engine's: flipped, it tells the engine,
/// and shows what the engine says next. `snapsBack` for a switch iOS keeps
/// (alerts), which goes back until the engine hears otherwise.
struct SwitchRow: View {
    let label: String
    let on: Bool
    var snapsBack = false
    let change: @MainActor (Bool) -> Void

    @State private var shown = false

    var body: some View {
        Toggle(label, isOn: $shown)
            .tint(Palette.accent)
            .onChange(of: on, initial: true) { _, value in shown = value }
            .onChange(of: shown) { _, value in
                guard value != on else { return }
                change(value)
                if snapsBack {
                    shown = on
                }
            }
    }
}
