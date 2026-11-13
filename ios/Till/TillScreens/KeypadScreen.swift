import SwiftUI
import TillCore

/// Till's keypad, the point of sale of a till with no other
/// (docs/view-key-mode.md §7), modelled on Monokulo's web POS. It opens
/// from the shift's New sale (core.md decision 257): over the list, full
/// screen, with Close, on a phone or a narrow window; beside the list on a
/// wide iPad, with nothing to close. From the top: the optional reference,
/// the amount, the keys and Charge, so the keys sit between the field and
/// Charge and a tap meant for the field can't charge (decision 258). A
/// hardware keyboard types too: digits, the point, Delete, Escape to clear
/// and Return to charge; Return in the reference only puts the keyboard
/// away.
///
/// Each key keeps the iPhone's proportions, 113 by 84 points, between a
/// phone held in one hand's 56 points high and a counter iPad's 112; on a
/// wider screen the column narrows to keys of that shape rather than
/// stretching them.
struct KeypadScreen: View {
    let view: KeypadView
    /// Over the list, it closes back to it; beside the list it doesn't.
    var closes = true
    let send: Send

    @FocusState private var typing: Bool
    @FocusState private var keys: Bool
    @State private var reference = ""

    var body: some View {
        GeometryReader { box in
            let size = KeypadScreen.layout(width: box.size.width, height: box.size.height)
            VStack(spacing: 14) {
                referenceField
                Spacer(minLength: 0)
                amount
                keypad(height: size.key)
                if let problem = view.problem {
                    Text(problem)
                        .font(.subheadline)
                        .foregroundStyle(Palette.danger)
                        .fixedSize(horizontal: false, vertical: true)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                PrimaryPress(press: view.charge) { send(.charge) }
            }
            .frame(width: size.column)
            .padding(.top, 8)
            .padding(.bottom, 12)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        // The reference is at the top: the keyboard comes up over the keys
        // rather than squeezing them.
        .ignoresSafeArea(.keyboard, edges: .bottom)
        .background(Palette.ground)
        .navigationTitle(closes ? view.title : "")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            if closes {
                ToolbarItem(placement: .topBarLeading) {
                    Button {
                        send(.back)
                    } label: {
                        Label(view.close, systemImage: "xmark")
                            .labelStyle(.iconOnly)
                    }
                    .accessibilityLabel(view.close)
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

    /// A key's height, and the column's width, in a space `width` by
    /// `height`: the keys as tall as the iPhone's shape allows across the
    /// column and as the height left over the reference, the amount and
    /// Charge allows, between 56 and 112; the column no wider than three
    /// keys of that shape, except on a phone, which keeps its whole width.
    static func layout(width: CGFloat, height: CGFloat) -> (key: CGFloat, column: CGFloat) {
        let available = max(0, width - 32)
        let fromWidth = (available - 2 * keyGap) / 3 * keyShape
        let fromHeight = (height - 320) / 4
        let key = min(112, max(56, min(fromWidth, fromHeight)))
        let keys = 3 * key / keyShape + 2 * keyGap
        let column = min(available, max(keys, phoneColumn))
        return (key, column)
    }

    /// The iPhone's key: 84 points high for 113 wide.
    static let keyShape: CGFloat = 84.0 / 113.0
    static let keyGap: CGFloat = 10
    /// Up to this wide the column takes the whole width, as on a phone.
    static let phoneColumn: CGFloat = 360

    /// What a hardware key does: digits and the point type, Delete deletes,
    /// Escape clears and Return charges. Return in the reference field
    /// doesn't come here: the field has the keyboard then.
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
                    .foregroundStyle(Palette.link)
                    .frame(minWidth: minimumTarget, minHeight: minimumTarget)
                    .accessibilityLabel(clear.said)
            }
        }
    }

    private func keypad(height: CGFloat) -> some View {
        Grid(horizontalSpacing: KeypadScreen.keyGap, verticalSpacing: KeypadScreen.keyGap) {
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
                // Done only puts the keyboard away and gives the keys back
                // their focus; Charge is a tap of its own (decision 258).
                .onSubmit {
                    typing = false
                    keys = true
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
