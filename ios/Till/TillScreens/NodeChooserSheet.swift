import SwiftUI
import TillCore

/// The node's Change, as Fob's Node screen (owner, 2026-11-13: "Copy the
/// node selection dialog from Fob wallet"), over D4 or Settings: the node
/// in use with Fob's checks, checked as it opens and by the refresh at the
/// top, "Changing node" while the wallet changes; then Change node, each
/// other node checked with Use on the ones that pass and Remove on this
/// till's own; and Add your own node. Fob draws it on Android only: this is
/// its SwiftUI equivalent in Till's look. Every word is the engine's.
struct NodeChooserSheet: View {
    let chooser: NodeChooser
    let send: Send

    @State private var adding = false
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List {
                Section {
                    if let changing = chooser.changing {
                        NodeVerdictHeader(mark: .checking, title: chooser.changingTitle, address: changing)
                    } else if let current = chooser.current {
                        NodeVerdictHeader(mark: current.mark, title: current.verdict, address: current.address)
                        ForEach(Array(current.rows.enumerated()), id: \.offset) { _, row in
                            NodeCheckLineRow(row: row)
                        }
                    }
                } footer: {
                    if let problem = chooser.problem {
                        Text(problem)
                            .foregroundStyle(Palette.danger)
                    }
                }
                Section {
                    ForEach(chooser.choices, id: \.url) { choice in
                        NodeChoiceLine(
                            choice: choice,
                            use: { send(.setNode(url: choice.url)) },
                            remove: { send(.removeNode(url: choice.url)) }
                        )
                    }
                    Button {
                        adding = true
                    } label: {
                        Label(chooser.add.label, systemImage: "plus")
                            .font(.body.weight(.semibold))
                            .foregroundStyle(chooser.add.enabled ? Palette.link : Palette.secondary)
                    }
                    .disabled(!chooser.add.enabled)
                } header: {
                    Text(chooser.section)
                }
            }
            .listStyle(.insetGrouped)
            .scrollContentBackground(.hidden)
            .background(Palette.ground)
            .navigationTitle(chooser.title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Done") { dismiss() }
                }
                ToolbarItem(placement: .primaryAction) {
                    if chooser.refresh.busy {
                        ProgressView()
                            .accessibilityLabel("Checking")
                    } else {
                        Button {
                            send(.checkNodes)
                        } label: {
                            Image(systemName: "arrow.clockwise")
                        }
                        .disabled(!chooser.refresh.enabled)
                        .accessibilityLabel(chooser.refresh.label)
                    }
                }
            }
        }
        // Checked as it opens, as Fob's Node screen is.
        .task { send(.checkNodes) }
        .sheet(isPresented: $adding, onDismiss: { send(.closeNewNode) }) {
            AddNodeSheet(sheet: chooser.newNode, send: send) { url in
                adding = false
                send(.setNode(url: url))
            }
            .presentationDetents([.medium, .large])
        }
    }
}

/// A check's mark: a filled circle with a tick or a "!", the system's
/// spinner while it runs, or a dash when it wasn't run.
struct NodeMarkCircle: View {
    let mark: NodeMark
    var size: CGFloat = 20

    var body: some View {
        switch mark {
        case .pass:
            symbol("checkmark.circle.fill", Palette.paid)
        case .fail:
            symbol("exclamationmark.circle.fill", Palette.danger)
        case .checking:
            ProgressView()
                .controlSize(size > 30 ? .regular : .small)
                .frame(width: size, height: size)
        case .skipped:
            symbol("minus.circle.fill", Palette.secondary)
        }
    }

    private func symbol(_ name: String, _ colour: Color) -> some View {
        Image(systemName: name)
            .resizable()
            .scaledToFit()
            .frame(width: size, height: size)
            .foregroundStyle(colour)
    }
}

/// The card's head: the verdict (or "Changing node") with its mark, and
/// the address under it, whole.
private struct NodeVerdictHeader: View {
    let mark: NodeMark
    let title: String
    let address: String

    var body: some View {
        HStack(spacing: 12) {
            NodeMarkCircle(mark: mark, size: 40)
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.title2.weight(.semibold))
                Text(address)
                    .font(.footnote)
                    .foregroundStyle(Palette.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.vertical, 6)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isHeader)
    }
}

/// One check: its mark and title, the result at the end, a second line
/// under it ("1.8 MB/s" under "31 blocks/s").
struct NodeCheckLineRow: View {
    let row: NodeCheckLine

    var body: some View {
        HStack(spacing: 12) {
            NodeMarkCircle(mark: row.mark)
            Text(row.title)
                .font(.subheadline)
            Spacer(minLength: 8)
            VStack(alignment: .trailing, spacing: 0) {
                Text(row.result)
                if let second = row.second {
                    Text(second)
                }
            }
            .font(.footnote)
            .foregroundStyle(row.mark == .fail ? Palette.danger : Palette.secondary)
            .multilineTextAlignment(.trailing)
            // The result keeps its words whole; the title wraps first.
            .fixedSize(horizontal: true, vertical: false)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(row.label)
    }
}

/// Another node: its dot, address and one line, Use when it passed, and
/// Remove when it's this till's own.
private struct NodeChoiceLine: View {
    let choice: NodeChoiceRow
    let use: @MainActor () -> Void
    let remove: @MainActor () -> Void

    var body: some View {
        HStack(spacing: 12) {
            Circle()
                .fill(dot)
                .frame(width: 10, height: 10)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(choice.address)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Text(choice.line)
                    .font(.footnote)
                    .foregroundStyle(Palette.secondary)
            }
            .accessibilityElement(children: .combine)
            Spacer(minLength: 8)
            if let changing = choice.changing {
                Text(changing)
                    .font(.subheadline)
                    .foregroundStyle(Palette.secondary)
            } else if let press = choice.useNode {
                Button(press.label) { use() }
                    .buttonStyle(.borderless)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(Palette.link)
                    .frame(minHeight: minimumTarget)
                    .disabled(!press.enabled)
                    .accessibilityLabel(choice.useSaid)
            }
            if choice.remove {
                Button {
                    remove()
                } label: {
                    Image(systemName: "trash")
                        .frame(width: minimumTarget, height: minimumTarget)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.borderless)
                .foregroundStyle(Palette.secondary)
                .accessibilityLabel(choice.removeSaid)
            }
        }
    }

    private var dot: Color {
        switch choice.mark {
        case .pass: Palette.paid
        case .fail: Palette.danger
        default: Palette.outline
        }
    }
}

/// Add your own node (Fob's board 12): an address and the same checks;
/// Use, or Use it anyway when only Up to date failed. The checks show while
/// the field still says what was checked; closing keeps the node if it
/// passed (the presenter's `onDismiss` sends `closeNewNode`).
private struct AddNodeSheet: View {
    let sheet: NewNodeSheet
    let send: Send
    let use: @MainActor (_ url: String) -> Void

    @State private var address = ""
    @Environment(\.dismiss) private var dismiss

    private var typed: String {
        address.trimmingCharacters(in: .whitespaces)
    }

    /// What was checked, while the field still says it.
    private var report: NodeReport? {
        guard !sheet.checked.isEmpty, sheet.checked == typed else { return nil }
        return sheet.report
    }

    var body: some View {
        NavigationStack {
            List {
                Section {
                    TextField(sheet.fieldLabel, text: $address)
                        .font(.body.monospaced())
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .keyboardType(.URL)
                        .submitLabel(.go)
                        .onSubmit { check() }
                } footer: {
                    Text(sheet.hint)
                }
                if let report {
                    Section {
                        ForEach(Array(report.rows.enumerated()), id: \.offset) { _, row in
                            NodeCheckLineRow(row: row)
                        }
                    }
                }
                Section {
                    if report != nil {
                        HStack(spacing: 12) {
                            Button(sheet.checkAgain.label) { check() }
                                .buttonStyle(.secondary)
                                .disabled(!sheet.checkAgain.enabled)
                            Button(sheet.useNode.label) { use(typed) }
                                .buttonStyle(.primary)
                                .disabled(!sheet.useNode.enabled)
                        }
                    } else {
                        Button(sheet.check) { check() }
                            .buttonStyle(.primary)
                            .disabled(typed.isEmpty)
                    }
                }
                .listRowBackground(Color.clear)
                .listRowInsets(EdgeInsets())
            }
            .listStyle(.insetGrouped)
            .scrollContentBackground(.hidden)
            .background(Palette.ground)
            .navigationTitle(sheet.title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(sheet.close) { dismiss() }
                }
            }
        }
    }

    private func check() {
        guard !typed.isEmpty else { return }
        send(.checkNewNode(url: address))
    }
}
