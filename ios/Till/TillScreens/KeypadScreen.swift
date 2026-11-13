import SwiftUI
import TillCore

/// Till's keypad, the home of a till with no other point of sale
/// (docs/view-key-mode.md §7), modelled on Monokulo's web POS: the sales put
/// aside for the next customer over the amount, the keys, an optional
/// reference and Charge. "Sales" opens the shift's list; Settings is in the
/// toolbar as on the shift. A hardware keyboard types too: digits, the
/// point, Delete, Escape to clear and Return to charge.
///
/// The keys share the height left under the amount, between a phone held in
/// one hand's 60 points and a counter iPad's 96, in a column no wider than
/// a hand spans on iPad.
struct KeypadScreen: View {
    let view: KeypadView
    let send: Send

    @FocusState private var typing: Bool
    @FocusState private var keys: Bool
    @State private var reference = ""

    var body: some View {
        GeometryReader { box in
            VStack(spacing: 14) {
                if !view.aside.isEmpty {
                    AsideStrip(title: view.asideTitle, sales: view.aside, send: send)
                }
                Spacer(minLength: 0)
                amount
                keypad(height: KeypadScreen.keyHeight(box.size.height, aside: !view.aside.isEmpty))
                referenceField
                if let problem = view.problem {
                    Text(problem)
                        .font(.subheadline)
                        .foregroundStyle(Palette.danger)
                        .fixedSize(horizontal: false, vertical: true)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                PrimaryPress(press: view.charge) { send(.charge) }
            }
            .frame(maxWidth: 520)
            .padding(.horizontal, 16)
            .padding(.bottom, 12)
            .frame(maxWidth: .infinity)
        }
        .background(Palette.ground)
        .navigationTitle("Till")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarLeading) {
                Button(view.sales.label) { send(.openSales) }
            }
            ToolbarItem(placement: .topBarTrailing) {
                Button {
                    send(.openSettings)
                } label: {
                    Label(view.settings, systemImage: "gearshape")
                }
            }
        }
        .focusable()
        .focused($keys)
        .focusEffectDisabled()
        .onKeyPress(phases: .down) { press in
            guard !typing, let action = KeypadScreen.action(press.key, characters: press.characters) else { return .ignored }
            send(action)
            return .handled
        }
        .onAppear { keys = true }
        .onChange(of: view.reference, initial: true) { _, text in
            if text != reference { reference = text }
        }
    }

    /// Each key's height for a screen `height` tall.
    static func keyHeight(_ height: CGFloat, aside: Bool) -> CGFloat {
        let left = height - (aside ? 330 : 250)
        return min(96, max(56, left / 4 - 10))
    }

    /// What a hardware key does: digits and the point type, Delete deletes,
    /// Escape clears and Return charges.
    static func action(_ key: KeyEquivalent, characters: String) -> Action? {
        switch key {
        case .delete, .deleteForward: return .keypadPress(key: .backspace)
        case .escape: return .keypadPress(key: .clear)
        case .return: return .charge
        default: break
        }
        if let digit = characters.first?.wholeNumberValue, characters.count == 1, digit <= 9 {
            return .keypadPress(key: .digit(digit: UInt8(digit)))
        }
        if characters == "." || characters == "," {
            return .keypadPress(key: .point)
        }
        return nil
    }

    private var amount: some View {
        HStack(spacing: 8) {
            Text(view.amount)
                .font(.system(size: 56, weight: .semibold, design: .rounded).monospacedDigit())
                .minimumScaleFactor(0.4)
                .lineLimit(1)
                .contentTransition(.numericText())
                .animation(.snappy(duration: 0.15), value: view.amount)
                .frame(maxWidth: .infinity)
                .accessibilityLabel(view.amountSaid)
                .accessibilityAddTraits(.updatesFrequently)
            if let clear = view.clear {
                Button(clear.label) { send(.keypadPress(key: clear.key)) }
                    .font(.title3.weight(.semibold))
                    .frame(minWidth: minimumTarget, minHeight: minimumTarget)
                    .accessibilityLabel(clear.said)
            }
        }
    }

    private func keypad(height: CGFloat) -> some View {
        Grid(horizontalSpacing: 10, verticalSpacing: 10) {
            ForEach(0..<4, id: \.self) { row in
                GridRow {
                    ForEach(0..<3, id: \.self) { column in
                        let index = row * 3 + column
                        if index < view.keys.count {
                            KeyButton(key: view.keys[index], height: height) { send(.keypadPress(key: view.keys[index].key)) }
                        }
                    }
                }
            }
        }
    }

    private var referenceField: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(view.referenceLabel)
                .font(.footnote)
                .foregroundStyle(Palette.secondary)
            TextField(view.referencePlaceholder, text: $reference)
                .textFieldStyle(.plain)
                .padding(.horizontal, 14)
                .frame(minHeight: 48)
                .background(RoundedRectangle(cornerRadius: 12).fill(Palette.card))
                .overlay(RoundedRectangle(cornerRadius: 12).stroke(Palette.border))
                .submitLabel(.done)
                .focused($typing)
                .onChange(of: reference) { _, text in
                    let kept = String(text.prefix(Int(view.referenceMax)))
                    if kept != text { reference = kept }
                    if kept != view.reference { send(.keypadReference(text: kept)) }
                }
                .onSubmit {
                    typing = false
                    keys = true
                    send(.charge)
                }
        }
    }
}

/// One key: its label large on a card, ⌫ as the system's delete symbol.
struct KeyButton: View {
    let key: KeypadButton
    let height: CGFloat
    let action: @MainActor () -> Void

    var body: some View {
        Button(action: action) {
            Group {
                if key.key == .backspace {
                    Image(systemName: "delete.left")
                        .font(.title2.weight(.medium))
                } else {
                    Text(key.label)
                        .font(.system(size: min(40, height * 0.45), weight: .medium, design: .rounded))
                }
            }
            .frame(maxWidth: .infinity)
            .frame(height: height)
            .background(RoundedRectangle(cornerRadius: 16, style: .continuous).fill(Palette.card))
            .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).stroke(Palette.border))
            .contentShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
        }
        .buttonStyle(KeyPressStyle())
        .foregroundStyle(Palette.ink)
        .accessibilityLabel(key.said)
    }
}

/// A key goes a little darker under the finger.
private struct KeyPressStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .opacity(configuration.isPressed ? 0.6 : 1)
    }
}

/// The sales put aside for the next customer, still live: one chip each,
/// sideways, opening the sale's code.
struct AsideStrip: View {
    let title: String?
    let sales: [AsideSale]
    let send: Send

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if let title {
                Text(title)
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(Palette.secondary)
            }
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    ForEach(sales, id: \.orderId) { sale in
                        Button {
                            send(.openSale(orderId: sale.orderId))
                        } label: {
                            VStack(alignment: .leading, spacing: 2) {
                                HStack(spacing: 6) {
                                    Text(sale.label)
                                        .font(.subheadline.weight(.semibold))
                                        .lineLimit(1)
                                    Text(sale.amount)
                                        .font(.subheadline.monospacedDigit())
                                        .lineLimit(1)
                                }
                                Text(sale.status)
                                    .font(.caption)
                                    .foregroundStyle(Palette.secondary)
                                    .lineLimit(1)
                            }
                            .padding(.horizontal, 12)
                            .padding(.vertical, 8)
                            .frame(minHeight: minimumTarget)
                            .background(RoundedRectangle(cornerRadius: 12).fill(Palette.card))
                            .overlay(RoundedRectangle(cornerRadius: 12).stroke(Palette.border))
                        }
                        .buttonStyle(.plain)
                        .foregroundStyle(Palette.ink)
                        .accessibilityElement(children: .ignore)
                        .accessibilityLabel(sale.said)
                    }
                }
            }
        }
        .padding(.top, 8)
    }
}
