import SwiftUI
import TillCore
import UIKit

/// Settings (D4; a sheet over the till with Done), drawn from the engine's
/// `SettingsView` as an inset grouped list: the store and its wallet (which
/// Share hands on), the point of sale, which device's sales alert and
/// whether other counters' show (C5), cash sales (K4), alerts as a status
/// row with Open Notification Settings (iOS decides; never a switch that
/// snaps back), how a new sale shows up (iPad), Wake-ups (its own page,
/// pushed here, as `Screen` has none), the Dynamic Island on iPhone, the
/// debug builds' stand-ins, and Disconnect. Section titles and
/// Disconnect's footnote are written once here (decision 26).
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
                    StoreSettings(store: store, send: send)
                }
                PosSettings(section: view.pointOfSale, send: send)
                if let counters = view.counters {
                    CountersSettings(section: counters, send: send)
                }
                if let cash = view.cash {
                    CashSettings(section: cash, send: send)
                }
                alertsSection
                if let showSales = view.showSales {
                    ShowSalesSettings(section: showSales, send: send)
                }
                wakeUpsSection
                if let island = view.island {
                    IslandSettings(island: island, send: send)
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
            .readableWidth(640)
            .background(Palette.ground)
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.large)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { send(.back) }
                }
            }
            .confirmationDialog("Disconnect this device?", isPresented: $askDisconnect, titleVisibility: .visible) {
                Button("Disconnect", role: .destructive) { send(.disconnect) }
            }
        }
        .tint(Palette.link)
    }

    /// Alerts as iOS allows them: the engine's status row where it gives
    /// one (`alertsStatus`), else from its switch's state.
    @ViewBuilder
    private var alertsSection: some View {
        if let status = view.alertsStatus {
            Section {
                AlertsStatusRow(row: status.row)
                if AlertsStatusRow.off(status.row) {
                    Button(status.openSettings) { openAlertSettings() }
                }
            } header: {
                Text("Alerts")
            } footer: {
                Text(status.footnote)
            }
        } else {
            Section {
                AlertsRow(on: view.alerts.on)
                if !view.alerts.on {
                    Button("Open Notification Settings") { openAlertSettings() }
                }
            } header: {
                Text("Alerts")
            } footer: {
                Text(view.alerts.footnote)
            }
        }
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

/// Whether iOS lets Till alert: "On", or "Off" in the warn colour with its
/// glyph, so it doesn't rest on colour alone.
struct AlertsRow: View {
    let on: Bool

    var body: some View {
        LabeledContent("New sale alerts") {
            if on {
                Text("On")
            } else {
                Label("Off", systemImage: "exclamationmark.triangle")
                    .labelStyle(.titleAndIcon)
                    .foregroundStyle(Palette.expiredInk)
            }
        }
    }
}

/// The engine's alerts row ("New sales and payments · On"): off, in the
/// warn colour with its glyph, so it doesn't rest on colour alone.
struct AlertsStatusRow: View {
    let row: Fact

    var body: some View {
        LabeledContent(row.label) {
            if AlertsStatusRow.off(row) {
                Label(row.value, systemImage: "exclamationmark.triangle")
                    .labelStyle(.titleAndIcon)
                    .foregroundStyle(toneColor(row.tone))
            } else {
                Text(row.value)
            }
        }
    }

    /// The engine warns: alerts are off.
    static func off(_ row: Fact) -> Bool {
        row.tone == .warn
    }
}

/// Settings › Alert for and Other counters (CounterSettings, C5): this
/// device or every device at the location, finding this device again with
/// a test sale, and whether other counters' sales show in the list.
struct CountersSettings: View {
    let section: CountersSection
    let send: Send

    var body: some View {
        Section {
            if let this = section.thisDevice {
                ChoiceRow(choice: this) { send(.setAlertFor(everyDevice: false)) }
            }
            ChoiceRow(choice: section.everyDevice) { send(.setAlertFor(everyDevice: true)) }
            Button(section.findAgain.label) { send(.findDevice) }
                .disabled(!section.findAgain.enabled)
        } header: {
            Text("Alert for")
        } footer: {
            Text(section.footnote)
        }
        Section {
            SwitchRow(label: CountersSettings.showOthers, on: section.showOthers) {
                send(.setShowOtherCounters(on: $0))
            }
        } header: {
            Text("Other counters")
        } footer: {
            Text(section.showOthersFootnote)
        }
    }

    static let showOthers = "Show their sales in the list"
}

/// Settings › Cash in Square (CashSettings, K4): cash sales treated as
/// Monero, or only offered.
struct CashSettings: View {
    let section: CashSection
    let send: Send

    var body: some View {
        Section {
            ChoiceRow(choice: section.asMonero) { send(.setCashAsMonero(on: true)) }
            ChoiceRow(choice: section.offer) { send(.setCashAsMonero(on: false)) }
        } header: {
            Text("Cash in Square")
        } footer: {
            Text(section.footnotes.joined(separator: "\n\n"))
        }
    }
}

/// Settings › How sales show up, on iPad: an alert, a floating window, or
/// side by side, with how to arrange side by side while it's chosen.
struct ShowSalesSettings: View {
    let section: ShowSalesSection
    let send: Send

    var body: some View {
        Section {
            ForEach(Array(section.choices.enumerated()), id: \.offset) { _, choice in
                ChoiceRow(choice: choice.choice) { send(.chooseShowSales(mode: choice.mode)) }
            }
        } header: {
            Text("How sales show up")
        } footer: {
            if let how = section.sideBySide {
                Text(how)
            }
        }
    }
}

/// Settings › Dynamic Island, on iPhone (PhoneSettings): the Live Activity
/// each shift, and "Show code in island", off by default, which first says
/// that each sale's payment address then goes through the push relay
/// (design.md §6). The words are the board's.
struct IslandSettings: View {
    let island: IslandSection
    let send: Send

    @State private var askCode = false

    var body: some View {
        Section {
            SwitchRow(label: "Live Activity", on: island.liveActivity) { send(.setLiveActivity(on: $0)) }
            // Turned on, it asks first, and springs back until the engine hears yes.
            SwitchRow(label: "Show code in island", on: island.codeInIsland, snapsBack: !island.codeInIsland) { on in
                if on {
                    askCode = true
                } else {
                    send(.setCodeInIsland(on: false))
                }
            }
            .disabled(!island.liveActivity)
        } header: {
            Text("Dynamic Island")
        } footer: {
            Text(IslandSettings.footnote)
        }
        .alert("Show the code in the island?", isPresented: $askCode) {
            Button("Show code") { send(.setCodeInIsland(on: true)) }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(IslandSettings.ask)
        }
    }

    static let footnote = "Showing the code in the island sends each sale's payment address through the Till push service, which otherwise only sees amount and status. Live Activities end after 8 hours: start Till again each shift."

    static let ask = "Each sale's payment address will pass through the Till push service. The island's code has no receipt: the customer's wallet won't show what was bought."
}

/// The store, its wallet and this device.
struct StoreSettings: View {
    let store: StoreSection
    let send: Send

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
                        Image(systemName: store.online ? "circle.fill" : "circle.dashed")
                            .font(.system(size: 8, weight: .bold))
                            .foregroundStyle(store.online ? Palette.ink : Palette.secondary)
                        Text(store.onlineLabel)
                            .font(.footnote.weight(.semibold))
                            .foregroundStyle(store.online ? Palette.ink : Palette.secondary)
                    }
                    .accessibilityElement(children: .combine)
                }
            }
            .padding(.vertical, 4)
            ForEach(Array(store.facts.enumerated()), id: \.offset) { _, fact in
                if fact.mono, let address = store.walletAddress {
                    HStack(alignment: .center, spacing: 8) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(fact.label)
                            Text(fact.value)
                                .font(.footnote.monospaced())
                                .foregroundStyle(toneColor(fact.tone))
                                .lineLimit(1)
                                .truncationMode(.middle)
                        }
                        .accessibilityElement(children: .combine)
                        Spacer(minLength: 8)
                        // Share hands the address on: Copy, AirDrop, Messages.
                        ShareLink(item: address) {
                            Image(systemName: "square.and.arrow.up")
                                .font(.body.weight(.semibold))
                                .frame(width: minimumTarget, height: minimumTarget)
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.borderless)
                        .accessibilityLabel("Share the wallet address")
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

/// A radio row: its label and line, ticked when chosen. `padded` on a
/// card of Till's own, where no list pads it, so the padding taps too.
struct ChoiceRow: View {
    let choice: Choice
    var padded = false
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
            .padding(.horizontal, padded ? 16 : 0)
            .padding(.vertical, padded ? 10 : 0)
            .frame(minHeight: padded ? 60 : nil)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!choice.enabled)
        .accessibilityAddTraits(choice.selected ? .isSelected : [])
    }
}

/// A switch row whose state is the engine's: flipped, it tells the engine,
/// and shows what the engine says next. `snapsBack` for a switch that asks
/// first, which goes back until the engine hears yes.
struct SwitchRow: View {
    let label: String
    let on: Bool
    var snapsBack = false
    let change: @MainActor (Bool) -> Void

    @State private var shown = false

    var body: some View {
        Toggle(label, isOn: $shown)
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
