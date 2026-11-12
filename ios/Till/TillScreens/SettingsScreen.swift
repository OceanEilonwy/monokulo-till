import SwiftUI
import TillCore
import UIKit

/// Settings (D4; a sheet over the till with Done), drawn from the engine's
/// `SettingsView` as an inset grouped list, in the order core.md decision
/// 182 sets: what the cashier changes most first, the setup under it, the
/// store, and Disconnect last.
///
/// 1. Watch for sales, a switch with no header.
/// 2. Notifications: iOS's status row and Open Notification Settings (iOS
///    decides; never a switch that snaps back).
/// 3. How sales show up (iPad), the rows O4 draws, with their pictures; or
///    the Dynamic Island (iPhone).
/// 4. Point of sale, then Alert for and Other counters (C5), then Cash in
///    Square (K4), one switch.
/// 5. Wake-ups, a row to its own page (`Screen` has none).
/// 6. Store, its wallet (which Share hands on) and this device.
/// 7. The debug builds' stand-ins, then Disconnect.
///
/// Section titles and Disconnect's words are the engine's too (core.md
/// decision 130).
struct SettingsScreen: View {
    let view: SettingsView
    let wakeUps: WakeUpsView?
    let send: Send

    @Environment(\.openURL) private var openURL
    @State private var askDisconnect = false

    var body: some View {
        NavigationStack {
            List {
                if let shift = view.shift {
                    ShiftSettings(section: shift, send: send)
                }
                alertsSection
                if let showSales = view.showSales {
                    ShowSalesSettings(section: showSales, send: send)
                }
                if let island = view.island {
                    IslandSettings(island: island, send: send)
                }
                PosSettings(section: view.pointOfSale, title: view.posTitle, send: send)
                if let counters = view.counters {
                    CountersSettings(section: counters, send: send)
                }
                if let cash = view.cash {
                    CashSettings(section: cash, send: send)
                }
                wakeUpsSection
                if let store = view.store {
                    StoreSettings(store: store, title: view.storeTitle, send: send)
                }
                if !view.standIns.isEmpty {
                    Section(view.developerTitle) {
                        ForEach(Array(view.standIns.enumerated()), id: \.offset) { _, standIn in
                            StandInField(standIn: standIn) { url in
                                send(.setBaseUrl(pos: pos(standIn.pointOfSale), url: url))
                            }
                        }
                    }
                }
                Section {
                    Button(view.disconnect, role: .destructive) { askDisconnect = true }
                        .foregroundStyle(Palette.danger)
                } footer: {
                    Text(view.disconnectFootnote)
                }
            }
            .listStyle(.insetGrouped)
            .scrollContentBackground(.hidden)
            .readableWidth(640)
            .background(Palette.ground)
            .navigationTitle(view.title)
            .navigationBarTitleDisplayMode(.large)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { send(.back) }
                }
            }
            .confirmationDialog(view.disconnectAsk, isPresented: $askDisconnect, titleVisibility: .visible) {
                Button(view.disconnectConfirm, role: .destructive) { send(.disconnect) }
            }
        }
        .tint(Palette.link)
    }

    /// Notifications as iOS allows them, in a section of their own: the
    /// engine's status row, which it gives on iOS (`alertsStatus`).
    @ViewBuilder
    private var alertsSection: some View {
        if let status = view.alertsStatus {
            Section {
                AlertsStatusRow(row: status.row)
                if AlertsStatusRow.off(status.row) {
                    Button(status.openSettings) { openAlertSettings() }
                }
            } header: {
                Text(view.alertsTitle)
            } footer: {
                Text(status.footnote)
            }
        }
    }

    @ViewBuilder
    private var wakeUpsSection: some View {
        switch view.wakeUps {
        case .row(let row):
            // The row names itself ("Wake-ups · Monokulo Bell"), as a
            // Settings row that opens a page does; no header to repeat it.
            Section {
                NavigationLink {
                    WakeUpsScreen(view: wakeUps, title: view.wakeUpsTitle, send: send)
                } label: {
                    LabeledContent(row.label, value: row.value)
                        .lineLimit(1)
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
            Text(section.title)
        } footer: {
            Text(section.footnote)
        }
        Section {
            SwitchRow(label: section.showOthersLabel, on: section.showOthers) {
                send(.setShowOtherCounters(on: $0))
            }
        } header: {
            Text(section.othersTitle)
        } footer: {
            Text(section.showOthersFootnote)
        }
    }
}

/// Settings › Cash in Square (CashSettings, K4): one switch, on by
/// default, cash sales treated as Monero; the footer says what it does as
/// it stands (core.md decision 182).
struct CashSettings: View {
    let section: CashSection
    let send: Send

    var body: some View {
        Section {
            SwitchRow(label: section.label, on: section.on) { send(.setCashAsMonero(on: $0)) }
        } header: {
            Text(section.title)
        } footer: {
            Text(section.footnote)
        }
    }
}

/// Settings › Watch for sales: Stop and Start watching, as on Android. The
/// shift's totals run from when it started (docs/ledger.md §9).
struct ShiftSettings: View {
    let section: SwitchSection
    let send: Send

    var body: some View {
        Section {
            SwitchRow(label: "Watch for sales", on: section.on) { on in
                send(on ? .startShift : .stopShift)
            }
        } footer: {
            Text(section.footnote)
        }
    }
}

/// Settings › How sales show up, on iPad: an alert or side by side, each led by O4's picture (``ShowSalesRow``), with how to
/// arrange side by side while it's chosen.
struct ShowSalesSettings: View {
    let section: ShowSalesSection
    let send: Send

    var body: some View {
        Section {
            ForEach(Array(section.choices.enumerated()), id: \.offset) { _, choice in
                ShowSalesRow(choice: choice) { send(.chooseShowSales(mode: choice.mode)) }
            }
        } header: {
            Text(section.title)
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
/// (design.md §6). The words are the board's, from the engine.
struct IslandSettings: View {
    let island: IslandSection
    let send: Send

    @State private var askCode = false

    var body: some View {
        Section {
            SwitchRow(label: island.liveActivityLabel, on: island.liveActivity) { send(.setLiveActivity(on: $0)) }
            // Turned on, it asks first, and springs back until the engine hears yes.
            SwitchRow(label: island.codeLabel, on: island.codeInIsland, snapsBack: !island.codeInIsland) { on in
                if on {
                    askCode = true
                } else {
                    send(.setCodeInIsland(on: false))
                }
            }
            .disabled(!island.liveActivity)
        } header: {
            Text(island.title)
        } footer: {
            Text(island.footnote)
        }
        .alert(island.askTitle, isPresented: $askCode) {
            Button(island.askConfirm) { send(.setCodeInIsland(on: true)) }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(island.ask)
        }
    }
}

/// The store, its wallet and this device.
struct StoreSettings: View {
    let store: StoreSection
    /// "Store".
    let title: String
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
            Text(title)
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
    /// "Point of sale".
    let title: String
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
                Text(title)
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
                Text(title)
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
    /// "Wake-ups", from Settings.
    let title: String
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
        .navigationTitle(title)
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
                statusAction(status.action, openAlertSettings: status.openAlertSettings)
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
    private func statusAction(_ action: StatusAction?, openAlertSettings: String) -> some View {
        switch action {
        case .some(.openAlertSettings):
            Button(openAlertSettings) {
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
    /// An SF Symbol leading the row, where the choice has one.
    var icon: String? = nil
    let action: @MainActor () -> Void

    var body: some View {
        Button {
            action()
        } label: {
            HStack(spacing: 12) {
                if let icon {
                    Image(systemName: icon)
                        .font(.title3)
                        .foregroundStyle(Palette.ink)
                        .frame(width: 28)
                        .accessibilityHidden(true)
                }
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
