import SwiftUI
import TillCore

/// Settings › Payments for a till on the shop's own wallet (core.md
/// decision 234), in place of the store: how Till sees payments, Fob, the
/// wallet and this till; then the node and its checks; then Paid after,
/// the code's expiry and the price source. No canvas board draws it: the
/// rest of Settings' look, with D4's steppers, node row and checks.
struct WalletSettings: View {
    let wallet: WalletSection
    let send: Send

    @State private var changingNode = false

    var body: some View {
        Section {
            if let problem = wallet.problem {
                Text(problem)
                    .font(.subheadline)
                    .foregroundStyle(Palette.danger)
            }
            ForEach(Array(wallet.warnings.enumerated()), id: \.offset) { _, warning in
                WarningLine(text: warning)
            }
            FactLine(fact: wallet.mode)
            if let fob = wallet.fob {
                FactLine(fact: fob)
            }
            ForEach(Array(wallet.facts.enumerated()), id: \.offset) { _, fact in
                if fact.mono {
                    addressRow(fact)
                } else {
                    FactLine(fact: fact)
                }
            }
            if let number = wallet.tillNumber {
                StepperRow(stepper: number, inList: true) { send(.stepTillNumber(up: $0)) }
            } else if let account = wallet.account {
                FactLine(fact: account)
            }
            if let lookahead = wallet.lookahead {
                WarningLine(text: lookahead)
            }
        } header: {
            Text(wallet.title)
        }
        if let node = wallet.node {
            Section {
                nodeRow(node)
                if !node.checks.isEmpty {
                    FlowRow(spacing: 18, lineSpacing: 8) {
                        ForEach(Array(node.checks.enumerated()), id: \.offset) { _, check in
                            NodeCheck(check: check)
                        }
                    }
                    .padding(.vertical, 4)
                }
            }
            .sheet(isPresented: $changingNode) {
                NodeSheet(node: node) { url in
                    changingNode = false
                    send(.setNode(url: url))
                }
                .presentationDetents([.medium, .large])
            }
        }
        Section {
            StepperRow(stepper: wallet.paidAfter, inList: true) { send(.stepPaidAfter(up: $0)) }
            StepperRow(stepper: wallet.expiry, inList: true) { send(.stepCodeExpiry(up: $0)) }
            pricesRow
        } footer: {
            Text(wallet.prices.footnote + "\n\n" + wallet.footnote)
        }
    }

    /// The address, short and monospaced, and Copy.
    private func addressRow(_ fact: Fact) -> some View {
        HStack(spacing: 8) {
            LabeledContent(fact.label) {
                Text(fact.value)
                    .font(.body.monospaced())
                    .foregroundStyle(toneColor(fact.tone))
                    .lineLimit(1)
            }
            Button {
                send(.copyWalletAddress)
            } label: {
                Image(systemName: "doc.on.doc")
                    .font(.body.weight(.semibold))
                    .frame(width: minimumTarget, height: minimumTarget)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.borderless)
            .accessibilityLabel("Copy the wallet address")
        }
    }

    private func nodeRow(_ node: NodeRow) -> some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(node.label)
                Text(node.value)
                    .font(.subheadline)
                    .foregroundStyle(Palette.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            .accessibilityElement(children: .combine)
            Spacer(minLength: 8)
            Button(node.change.label) { changingNode = true }
                .buttonStyle(.borderless)
                .disabled(!node.change.enabled)
                .accessibilityLabel("\(node.change.label) the node")
        }
    }

    /// Prices from, as the system's menu of the price sources, ticked.
    private var pricesRow: some View {
        Menu {
            ForEach(Array(wallet.prices.choices.enumerated()), id: \.offset) { _, choice in
                Button {
                    send(.choosePriceProvider(name: choice.label))
                } label: {
                    if choice.selected {
                        Label(choice.label, systemImage: "checkmark")
                    } else {
                        Text(choice.label)
                    }
                }
                .disabled(!choice.enabled)
            }
        } label: {
            LabeledContent(wallet.prices.label) {
                HStack(spacing: 4) {
                    Text(wallet.prices.value)
                    Image(systemName: "chevron.up.chevron.down")
                        .font(.footnote.weight(.semibold))
                        .accessibilityHidden(true)
                }
                .foregroundStyle(Palette.secondary)
            }
            .foregroundStyle(Palette.ink)
        }
    }
}

/// A line the wallet warns with: a triangle and the words, in the warn
/// colour, so it doesn't rest on colour alone.
struct WarningLine: View {
    let text: String

    var body: some View {
        Label {
            Text(text)
                .font(.subheadline)
                .fixedSize(horizontal: false, vertical: true)
        } icon: {
            Image(systemName: "exclamationmark.triangle")
        }
        .foregroundStyle(Palette.expiredInk)
    }
}
