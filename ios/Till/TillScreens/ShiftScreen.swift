import SwiftUI
import TillCore

/// The shift, the till's home (PadShift, PhoneShift; built from the
/// design's ShiftList), drawn from the engine's `ShiftView`: the store and
/// whether Till is watching, the summary, the filter chips with Needs
/// action's count, and the sales, the waiting tray first. Tapping a row
/// opens its sale (its code while live, else its page); a live row swipes
/// to the close sheet.
struct ShiftScreen: View {
    let view: ShiftView
    let send: Send

    var body: some View {
        VStack(spacing: 0) {
            ShiftTopBar(top: view.top) { send(.openSettings) }
            if let disconnected = view.disconnected {
                DisconnectedPanel(disconnected: disconnected) { send(.forget) }
            } else {
                list
            }
        }
        .background(Palette.ground)
    }

    private var list: some View {
        List {
            Group {
                if let problem = view.problem {
                    ProblemNote(text: problem)
                        .padding(.horizontal, -16)
                        .listRowInsets(ShiftEntryRow.padded)
                }
                if let again = view.signInAgain {
                    PrimaryPress(press: again) { send(.signIn(applicationId: nil)) }
                        .padding(.vertical, 4)
                        .listRowInsets(ShiftEntryRow.padded)
                }
                VStack(alignment: .leading, spacing: 4) {
                    Text("This shift")
                        .font(.footnote)
                        .foregroundStyle(Palette.secondary)
                        .accessibilityAddTraits(.isHeader)
                    Text(view.summary)
                        .font(.headline)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .padding(.top, 8)
                .listRowInsets(ShiftEntryRow.padded)
                if let idle = view.idle {
                    IdlePanel(idle: idle) { send(.startShift) }
                        .listRowInsets(ShiftEntryRow.padded)
                }
                if !view.chips.isEmpty {
                    FilterChips(chips: view.chips) { send(.chooseFilter(filter: $0)) }
                        .listRowInsets(ShiftEntryRow.padded)
                }
                ForEach(Array(view.entries.enumerated()), id: \.offset) { _, entry in
                    ShiftEntryRow(entry: entry, send: send)
                        .listRowInsets(ShiftEntryRow.insets(entry))
                }
                if let empty = view.empty {
                    Text(empty)
                        .font(.subheadline)
                        .foregroundStyle(Palette.secondary)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 24)
                        .listRowInsets(ShiftEntryRow.padded)
                }
            }
            .listRowBackground(Color.clear)
            .listRowSeparator(.hidden)
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .environment(\.defaultMinListRowHeight, 0)
    }
}

/// The top row: the store and its place, Watching in its colour, and
/// Settings.
struct ShiftTopBar: View {
    let top: ShiftTop
    let settings: @MainActor () -> Void

    var body: some View {
        HStack(spacing: 10) {
            Logo(size: 30)
            VStack(alignment: .leading, spacing: 2) {
                if let place = top.place {
                    Text(place)
                        .font(.subheadline.weight(.semibold))
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                HStack(spacing: 6) {
                    Circle()
                        .fill(Roles.ink(top.tone))
                        .frame(width: 8, height: 8)
                    Text(top.state)
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(Roles.ink(top.tone))
                }
            }
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(top.said)
            Spacer(minLength: 8)
            Button {
                settings()
            } label: {
                Image(systemName: "gearshape")
                    .font(.body.weight(.semibold))
            }
            .glassButton()
            .accessibilityLabel("Settings")
        }
        .padding(.horizontal, 16)
        .padding(.top, 8)
        .padding(.bottom, 4)
        .foregroundStyle(Palette.ink)
    }
}

/// No sale yet, or not watching: the faded mark, what happens next, and
/// "Start watching" while no shift is on.
struct IdlePanel: View {
    let idle: ShiftIdle
    let start: @MainActor () -> Void

    var body: some View {
        VStack(spacing: 12) {
            Logo(size: 72)
                .opacity(0.35)
            Text(idle.title)
                .font(.headline)
                .multilineTextAlignment(.center)
            Text(idle.hint)
                .font(.subheadline)
                .foregroundStyle(Palette.secondary)
                .multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
            if let press = idle.start {
                PrimaryPress(press: press, action: start)
                    .padding(.top, 8)
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 32)
    }
}

/// The filter chips: All, Needs action with its count, Paid.
struct FilterChips: View {
    let chips: [FilterChip]
    let choose: @MainActor (ShiftFilter) -> Void

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(Array(chips.enumerated()), id: \.offset) { _, chip in
                    Button {
                        choose(chip.filter)
                    } label: {
                        HStack(spacing: 6) {
                            Text(chip.label)
                                .font(.subheadline.weight(.semibold))
                            if let count = chip.count {
                                Text(String(count))
                                    .font(.caption.weight(.bold))
                                    .monospacedDigit()
                                    .foregroundStyle(Palette.onAccent)
                                    .padding(.horizontal, 6)
                                    .frame(minWidth: 20, minHeight: 20)
                                    .background(Capsule().fill(Palette.accent))
                            }
                        }
                        .foregroundStyle(chip.selected ? Palette.card : Palette.ink)
                        .padding(.horizontal, 14)
                        .frame(minHeight: 36)
                        .background(Capsule().fill(chip.selected ? Palette.ink : Palette.chip))
                        .frame(minHeight: minimumTarget)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityAddTraits(chip.selected ? .isSelected : [])
                }
            }
        }
        .padding(.vertical, 4)
    }
}

/// One item of the list: the tray's header, a sale's row, the tray's
/// fold, or Earlier.
struct ShiftEntryRow: View {
    let entry: ShiftEntry
    let send: Send

    var body: some View {
        switch entry {
        case .trayHeader(let label, let said, let count):
            // The tray's count of live sales beside its label (the engine's `count`).
            HStack(alignment: .firstTextBaseline) {
                Text(label)
                Spacer()
                Text("\(count)")
                    .monospacedDigit()
            }
            .font(.footnote.weight(.semibold))
            .foregroundStyle(Palette.secondary)
            .padding(.top, 12)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(said)
            .accessibilityAddTraits(.isHeader)
        case .row(let orderId, let row, let ends, _, let closes):
            SaleRow(row: row, ends: ends) { send(.openSale(orderId: orderId)) }
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
            .accessibilityLabel(said)
        case .earlier(let label):
            header(label)
        default:
            EmptyView()
        }
    }

    /// A row's insets: a sale's rows touch, so their section is one card.
    static let padded = EdgeInsets(top: 4, leading: 16, bottom: 4, trailing: 16)

    static func insets(_ entry: ShiftEntry) -> EdgeInsets {
        if case .row = entry {
            return EdgeInsets(top: 0, leading: 16, bottom: 0, trailing: 16)
        }
        return padded
    }

    private func header(_ label: String) -> some View {
        Text(label)
            .font(.footnote.weight(.semibold))
            .foregroundStyle(Palette.secondary)
            .padding(.top, 12)
            .accessibilityAddTraits(.isHeader)
    }
}

/// A sale's row: its state's badge, "Sale #m2Qe · $8.40" and how it
/// stands, on a card whose corners its place in the section rounds.
struct SaleRow: View {
    let row: ShiftRow
    let ends: RowEnds
    let open: @MainActor () -> Void

    var body: some View {
        Button {
            open()
        } label: {
            HStack(spacing: 12) {
                StateBadge(icon: row.icon, ground: Roles.badgeGround(row.icon), quiet: row.quiet)
                VStack(alignment: .leading, spacing: 2) {
                    Text(row.title)
                        .font(.body.weight(.semibold))
                        .foregroundStyle(row.quiet ? Palette.secondary : Palette.ink)
                        .lineLimit(1)
                    Text(row.sub)
                        .font(.subheadline)
                        .foregroundStyle(Roles.ink(row.tone))
                        .lineLimit(2)
                }
                Spacer(minLength: 4)
                Image(systemName: "chevron.forward")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(Palette.secondary)
                    .accessibilityHidden(true)
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            .frame(minHeight: 60)
            .background(RowShape(ends: ends).fill(Palette.card))
            .overlay(alignment: .top) {
                if ends == .middle || ends == .last {
                    Rectangle()
                        .fill(Palette.hairline)
                        .frame(height: 1)
                        .padding(.leading, 62)
                }
            }
            .contentShape(RowShape(ends: ends))
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .combine)
    }
}

/// A row's card: the rows of a section make one card, rounded at its ends.
struct RowShape: Shape {
    let ends: RowEnds
    var radius: CGFloat = 14

    func path(in rect: CGRect) -> Path {
        let top: CGFloat
        let bottom: CGFloat
        switch ends {
        case .only:
            top = radius
            bottom = radius
        case .first:
            top = radius
            bottom = 0
        case .last:
            top = 0
            bottom = radius
        default:
            top = 0
            bottom = 0
        }
        return UnevenRoundedRectangle(
            topLeadingRadius: top,
            bottomLeadingRadius: bottom,
            bottomTrailingRadius: bottom,
            topTrailingRadius: top,
            style: .continuous
        ).path(in: rect)
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
            Heading(text: disconnected.heading, size: 26, centred: true)
            Lead(text: disconnected.lead, centred: true)
            Spacer(minLength: 0)
            PrimaryPress(press: disconnected.connectAgain, action: connectAgain)
        }
        .padding(.horizontal, 24)
        .padding(.bottom, 16)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
