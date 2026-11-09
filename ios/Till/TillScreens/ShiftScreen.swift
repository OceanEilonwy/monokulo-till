import SwiftUI
import TillCore

/// The shift, the till's home (D1's and E2's sidebar, the iPhone's root),
/// drawn from the engine's `ShiftView`: the place as the toolbar's title
/// with whether Till is watching under it, Settings in the toolbar, then
/// the summary, the filter, and the sales in grouped sections, the waiting
/// tray first. Tapping a row opens its sale; a live row swipes to the close
/// sheet. With no sale yet the shift is the empty till (E1).
///
/// While this counter has a device, or sales come from more than one place,
/// the title is the counter shown and the system's title menu chooses whose
/// sales the list shows (C3, docs/ipad-redesign.md decision 5), on iPad and
/// iPhone alike.
struct ShiftScreen: View {
    let view: ShiftView
    let chips: [StatusChip]
    /// In the split view's sidebar, beside the open sale.
    let sidebar: Bool
    /// The sale open beside the sidebar, marked in the list.
    let selected: String?
    let send: Send

    var body: some View {
        content
            .navigationTitle(view.counters?.title ?? ShiftTitle.title(view.top))
            .navigationSubtitle(ShiftTitle.subtitle(view.top, menu: view.counters != nil))
            .modifier(CounterTitleMenu(menu: view.counters, send: send))
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        send(.openSettings)
                    } label: {
                        Label("Settings", systemImage: "gearshape")
                    }
                }
            }
            .background(Palette.ground)
    }

    @ViewBuilder
    private var content: some View {
        if let disconnected = view.disconnected {
            DisconnectedPanel(disconnected: disconnected) { send(.forget) }
        } else if view.chips.isEmpty && view.entries.isEmpty {
            EmptyTill(idle: view.idle, chips: chips, problem: view.problem, signInAgain: view.signInAgain, send: send)
        } else {
            list
        }
    }

    private var list: some View {
        List {
            if view.problem != nil || view.signInAgain != nil {
                Section {
                    ProblemNote(text: view.problem)
                        .padding(.horizontal, -16)
                    if let again = view.signInAgain {
                        PrimaryPress(press: again) { send(.signIn(applicationId: nil)) }
                    }
                }
                .listRowBackground(Color.clear)
                .listRowInsets(EdgeInsets(top: 4, leading: 0, bottom: 4, trailing: 0))
            }
            Section {
                LabeledContent("This shift") {
                    Text(view.summary)
                        .monospacedDigit()
                        .lineLimit(1)
                }
                // Square's books say Cash for these (decision 9).
                if let cash = view.cashInMonero {
                    Label {
                        Text(cash)
                            .monospacedDigit()
                            .lineLimit(1)
                    } icon: {
                        StatusGlyph(kind: .cash, size: 20)
                    }
                    .font(.subheadline)
                    .foregroundStyle(Palette.secondary)
                }
                if let idle = view.idle, let start = idle.start {
                    Button(start.label) { send(.startShift) }
                        .disabled(!start.enabled)
                }
            }
            if !view.chips.isEmpty {
                Section {
                    FilterPicker(chips: view.chips) { send(.chooseFilter(filter: $0)) }
                }
                .listRowBackground(Color.clear)
                .listRowInsets(EdgeInsets())
            }
            ForEach(Array(ShiftSections.of(view.entries).enumerated()), id: \.offset) { _, section in
                Section {
                    ForEach(Array(section.entries.enumerated()), id: \.offset) { _, entry in
                        entryRow(entry)
                    }
                } header: {
                    if let header = section.header {
                        SectionHeader(header: header)
                    }
                }
            }
            if let empty = view.empty {
                Section {
                    Text(empty)
                        .font(.subheadline)
                        .foregroundStyle(Palette.secondary)
                        .frame(maxWidth: .infinity)
                }
                .listRowBackground(Color.clear)
            }
        }
        .listStyle(.insetGrouped)
        .scrollContentBackground(.hidden)
        .readableWidth(sidebar ? .greatestFiniteMagnitude : readableColumn)
        .foregroundStyle(Palette.ink)
    }

    @ViewBuilder
    private func entryRow(_ entry: ShiftEntry) -> some View {
        switch entry {
        case .row(let orderId, let row, _, _, let closes):
            SaleRow(row: row, selected: selected == orderId) { send(.openSale(orderId: orderId)) }
                .listRowBackground(selected == orderId ? Palette.chip : Palette.card)
                .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                    if closes {
                        Button {
                            send(.askClose(orderId: orderId))
                        } label: {
                            Label("Close", systemImage: "xmark")
                        }
                        .tint(Palette.danger)
                    }
                }
        case .fold(let label, let said, let isOpen, _):
            Button {
                send(.foldTray(folded: isOpen))
            } label: {
                Text(label)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(Palette.link)
                    .frame(maxWidth: .infinity, minHeight: minimumTarget)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .listRowBackground(Palette.card)
            .accessibilityLabel(said)
        default:
            EmptyView()
        }
    }
}

/// The shift's title and its line, from the engine's top row: the
/// location as the title ("Front Counter"), and whether Till is watching
/// with the store under it ("Watching · The Bean Room").
enum ShiftTitle {
    static func title(_ top: ShiftTop) -> String {
        parts(top).last ?? "Till"
    }

    /// Under the location: whether Till watches, and the store. Under the
    /// title menu's counter: whether Till watches, and the whole place.
    static func subtitle(_ top: ShiftTop, menu: Bool = false) -> String {
        let place = menu ? parts(top) : Array(parts(top).dropLast())
        return ([top.state] + place).filter { !$0.isEmpty }.joined(separator: " · ")
    }

    /// The store and the location, as the engine joins them.
    private static func parts(_ top: ShiftTop) -> [String] {
        (top.place ?? "").components(separatedBy: " · ").filter { !$0.isEmpty }
    }
}

/// A section of the list: the tray or Earlier, under its header.
struct ShiftSection {
    struct Header: Equatable {
        let label: String
        var said: String? = nil
        var count: UInt32? = nil
    }

    var header: Header?
    var entries: [ShiftEntry]
}

enum ShiftSections {
    /// The engine's flat list as sections: each header starts one, and the
    /// rows and folds under it are its own.
    static func of(_ entries: [ShiftEntry]) -> [ShiftSection] {
        var sections: [ShiftSection] = []
        for entry in entries {
            switch entry {
            case .trayHeader(let label, let said, let count):
                sections.append(ShiftSection(header: .init(label: label, said: said, count: count), entries: []))
            case .earlier(let label):
                sections.append(ShiftSection(header: .init(label: label), entries: []))
            default:
                if sections.isEmpty {
                    sections.append(ShiftSection(header: nil, entries: []))
                }
                sections[sections.count - 1].entries.append(entry)
            }
        }
        return sections.filter { !$0.entries.isEmpty }
    }
}

/// A section's header: its label, and the tray's count beside it.
struct SectionHeader: View {
    let header: ShiftSection.Header

    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            Text(header.label)
                .lineLimit(1)
            Spacer()
            if let count = header.count {
                Text("\(count)")
                    .monospacedDigit()
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(header.said ?? header.label)
        .accessibilityAddTraits(.isHeader)
    }
}

/// All, Needs action with its count, Paid: the system's segmented control.
struct FilterPicker: View {
    let chips: [FilterChip]
    let choose: @MainActor (ShiftFilter) -> Void

    var body: some View {
        Picker("Show", selection: Binding(
            get: { chips.first { $0.selected }?.filter ?? .all },
            set: { choose($0) }
        )) {
            ForEach(Array(chips.enumerated()), id: \.offset) { _, chip in
                Text(FilterPicker.label(chip)).tag(chip.filter)
            }
        }
        .pickerStyle(.segmented)
    }

    /// "Needs action 2", with its count when there's one.
    static func label(_ chip: FilterChip) -> String {
        chip.count.map { "\(chip.label) \($0)" } ?? chip.label
    }
}

/// How a shift row reads, from the engine's fields: the customer's name
/// leading (else "Sale #m2Qe"), the status line in the engine's tone (green
/// for paid alone, decision 20), and the amount at the end, never cut.
enum SaleRowLook {
    /// What leads the row: the engine's name, else its one-line title.
    static func name(_ row: ShiftRow) -> String {
        row.name.isEmpty ? row.title : row.name
    }

    /// The status line's colour, as the engine tones it.
    static func subColour(_ row: ShiftRow) -> Color {
        Roles.ink(row.tone)
    }
}

/// A sale's row (CounterScope): its status glyph, the name and the status
/// line, each on one line, and the amount, with another counter's device
/// under it.
struct SaleRow: View {
    let row: ShiftRow
    var selected = false
    let open: @MainActor () -> Void

    var body: some View {
        Button {
            open()
        } label: {
            HStack(spacing: 12) {
                StatusGlyph(kind: StatusGlyphKind.of(row.icon, step: Int(row.ring.step)), size: 24)
                VStack(alignment: .leading, spacing: 2) {
                    Text(SaleRowLook.name(row))
                        .font(.body.weight(.semibold))
                        .foregroundStyle(row.quiet ? Palette.secondary : Palette.ink)
                        .lineLimit(1)
                    Text(row.sub)
                        .font(.footnote)
                        .foregroundStyle(SaleRowLook.subColour(row))
                        .monospacedDigit()
                        .lineLimit(1)
                }
                Spacer(minLength: 8)
                VStack(alignment: .trailing, spacing: 3) {
                    if !row.amount.isEmpty {
                        Text(row.amount)
                            .font(.body)
                            .monospacedDigit()
                            .foregroundStyle(row.quiet ? Palette.secondary : Palette.ink)
                    }
                    if let device = row.device {
                        Text(device)
                            .font(.caption)
                            .foregroundStyle(Palette.secondary)
                            .lineLimit(1)
                            .padding(.horizontal, 7)
                            .padding(.vertical, 1)
                            .background(Capsule().fill(Palette.chip))
                    }
                }
                .fixedSize()
                .layoutPriority(1)
            }
            .frame(minHeight: 48)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}

/// The shift's title menu (C3): the system's menu on the toolbar's title,
/// "Show sales from" each counter with its line and waiting count, the one
/// shown ticked, and "All counters" apart at the end. Nothing while the
/// engine has nothing to choose between.
struct CounterTitleMenu: ViewModifier {
    let menu: CounterMenu?
    let send: Send

    @ViewBuilder
    func body(content: Content) -> some View {
        if let menu {
            content.toolbarTitleMenu {
                Section(menu.heading) {
                    ForEach(Array(CounterTitleMenu.counters(menu).enumerated()), id: \.offset) { _, item in
                        toggle(item)
                    }
                }
                if let all = CounterTitleMenu.all(menu) {
                    Section {
                        toggle(all)
                    }
                }
            }
        } else {
            content
        }
    }

    private func toggle(_ item: CounterItem) -> some View {
        Toggle(isOn: Binding(
            get: { item.selected },
            set: { _ in send(.chooseCounter(choice: item.choice)) }
        )) {
            Text(item.name)
            if let line = CounterTitleMenu.line(item) {
                Text(line)
            }
        }
    }

    /// Every counter but "All counters".
    static func counters(_ menu: CounterMenu) -> [CounterItem] {
        menu.items.filter { $0.choice != .all }
    }

    static func all(_ menu: CounterMenu) -> CounterItem? {
        menu.items.first { $0.choice == .all }
    }

    /// An item's line in the menu: "This counter · alerts · 1 waiting".
    static func line(_ item: CounterItem) -> String? {
        let parts = [item.sub, item.count].compactMap { $0 }.filter { !$0.isEmpty }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }
}

/// One of the empty till's status chips: "Watching Square", "Alerts on",
/// "Instant with the Bell".
struct StatusChip: Equatable {
    enum Glyph: Equatable {
        case watching
        case symbol(String)
    }

    let glyph: Glyph
    let text: String
    /// Drawn in the warn colour: something is off.
    var warn = false

    /// The chips for what the engine publishes now: whether Till is
    /// watching, whether alerts are allowed, and whether a Bell wakes it.
    static func chips(top: ShiftTop?, alerts: Bool?, bell: Bool?) -> [StatusChip] {
        var chips: [StatusChip] = []
        if let top, !top.state.isEmpty {
            // The engine's watching tone: watching, and its point of sale named.
            let watching = top.tone == .paid
            let pos = watching ? top.mark.map { " " + StatusChip.name($0) } ?? "" : ""
            chips.append(StatusChip(glyph: watching ? .watching : .symbol("pause.circle"), text: top.state + pos))
        }
        if let alerts {
            chips.append(alerts
                ? StatusChip(glyph: .symbol("bell"), text: "Alerts on")
                : StatusChip(glyph: .symbol("bell.slash"), text: "Alerts off", warn: true))
        }
        if bell == true {
            chips.append(StatusChip(glyph: .symbol("bolt"), text: "Instant with the Bell"))
        }
        return chips
    }

    static func name(_ mark: PosMark) -> String {
        switch mark {
        case .square: "Square"
        case .clover: "Clover"
        }
    }
}

/// A row of status chips that stacks when it doesn't fit.
struct StatusChips: View {
    let chips: [StatusChip]

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 10) { items }
            VStack(spacing: 8) { items }
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel(chips.map(\.text).joined(separator: ", "))
    }

    private var items: some View {
        ForEach(Array(chips.enumerated()), id: \.offset) { _, chip in
            HStack(spacing: 8) {
                switch chip.glyph {
                case .watching:
                    StatusGlyph(kind: .watching, size: 18)
                case .symbol(let name):
                    Image(systemName: name)
                        .font(.subheadline)
                        .foregroundStyle(chip.warn ? Palette.expiredInk : Palette.ink)
                }
                Text(chip.text)
                    .font(.subheadline)
                    .foregroundStyle(chip.warn ? Palette.expiredInk : Palette.ink)
                    .lineLimit(1)
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 8)
            .background(Capsule().fill(Palette.card))
        }
    }
}

/// The empty till (E1, E2): one centred block, the Till logo idle, what
/// the engine says next and how a sale appears, the status chips, and
/// "Start watching" while no shift is on.
struct EmptyTill: View {
    let idle: ShiftIdle?
    let chips: [StatusChip]
    var problem: String? = nil
    var signInAgain: Press? = nil
    let send: Send

    /// E2's heading, beside a shift that has sales, where the engine has no
    /// idle panel to say it (docs/ipad-redesign.md, decision 15).
    static let ready = "Ready for the next sale"

    var body: some View {
        GeometryReader { box in
            ScrollView {
                VStack(spacing: 16) {
                    ProblemNote(text: problem)
                    if let signInAgain {
                        PrimaryPress(press: signInAgain) { send(.signIn(applicationId: nil)) }
                            .frame(maxWidth: 360)
                    }
                    Spacer(minLength: 0)
                    TillWidget(state: .idle, size: 128, decorative: true)
                    Text(idle?.title ?? EmptyTill.ready)
                        .font(.title.weight(.bold))
                        .multilineTextAlignment(.center)
                        .padding(.top, 8)
                        .accessibilityAddTraits(.isHeader)
                    if let hint = idle?.hint {
                        Text(hint)
                            .font(.body)
                            .foregroundStyle(Palette.secondary)
                            .multilineTextAlignment(.center)
                            .fixedSize(horizontal: false, vertical: true)
                            .frame(maxWidth: 440)
                    }
                    if !chips.isEmpty {
                        StatusChips(chips: chips)
                            .padding(.top, 12)
                    }
                    if let start = idle?.start {
                        PrimaryPress(press: start) { send(.startShift) }
                            .frame(maxWidth: 360)
                            .padding(.top, 8)
                    }
                    Spacer(minLength: 0)
                }
                .padding(24)
                .frame(maxWidth: .infinity, minHeight: box.size.height)
            }
        }
        .foregroundStyle(Palette.ink)
        .background(Palette.ground)
    }
}

/// The store disconnected this device: nothing else shows.
struct DisconnectedPanel: View {
    let disconnected: ShiftDisconnected
    let connectAgain: @MainActor () -> Void

    var body: some View {
        VStack(spacing: 14) {
            Spacer(minLength: 0)
            Image(systemName: "bolt.horizontal.circle")
                .font(.system(size: 48))
                .foregroundStyle(Palette.danger)
                .accessibilityHidden(true)
            Heading(text: disconnected.heading, centred: true)
            Lead(text: disconnected.lead, centred: true)
                .frame(maxWidth: 440)
            Spacer(minLength: 0)
            PrimaryPress(press: disconnected.connectAgain, action: connectAgain)
                .frame(maxWidth: 440)
        }
        .padding(.horizontal, 24)
        .padding(.bottom, 16)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
