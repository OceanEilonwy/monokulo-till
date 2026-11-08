import SwiftUI
import TillCore
import UIKit

/// A live sale (PadSale, PhoneSale; the design's SaleBody), drawn from the
/// engine's `SaleView`: the amount and the XMR (tapped, it's copied), the
/// code with its enlarge badge, how the payment stands, what was bought,
/// and Copy address. The bar's tinted X asks whether the customer is
/// paying. iOS can't open Square (docs/ios-plan.md §4), so there's no
/// "Back to Square". On iPhone (PhoneSale) Copy address stays at the foot,
/// under the scrolling body.
struct SaleScreen: View {
    let orderId: String
    let view: SaleView
    let send: Send
    var phone = false

    var body: some View {
        VStack(spacing: 0) {
            TillBar(title: view.title, line: view.titleLine, back: { send(.back) }) {
                if view.closes {
                    Button {
                        send(.askClose(orderId: orderId))
                    } label: {
                        Image(systemName: "xmark")
                            .font(.body.weight(.bold))
                            .foregroundStyle(Palette.danger)
                    }
                    .glassButton()
                    .tint(Palette.danger)
                    .accessibilityLabel("Close the sale")
                } else {
                    BarSpace()
                }
            }
            ScrollView {
                VStack(spacing: 16) {
                    VStack(spacing: 4) {
                        Text(view.amount)
                            .display(44, tracking: -1)
                            .minimumScaleFactor(0.5)
                            .lineLimit(1)
                        Button {
                            send(.copyAmount(orderId: orderId))
                        } label: {
                            Text(view.xmr)
                                .font(.subheadline.monospacedDigit())
                                .foregroundStyle(Palette.secondary)
                                .frame(minHeight: minimumTarget)
                        }
                        .buttonStyle(.plain)
                        .accessibilityHint("Copies the amount")
                    }
                    .padding(.top, 8)
                    CodeCard(code: view.code, said: view.codeSaid, spent: view.spent, size: phone ? 220 : 240) {
                        send(.enlarge(orderId: orderId))
                    }
                    SaleStatus(status: view.status, hint: view.hint, step: Int(view.step))
                    if let order = view.order {
                        OrderCardSection(card: order) { send(.showOrder(orderId: orderId)) }
                    }
                    if !phone {
                        copyAddress
                            .padding(.horizontal, 16)
                    }
                }
                .padding(.bottom, 24)
            }
            .scrollBounceBehavior(.basedOnSize)
            if phone {
                copyAddress
                    .padding(.horizontal, 20)
                    .padding(.top, 8)
                    .padding(.bottom, 8)
            }
        }
        .foregroundStyle(Palette.ink)
        .background(Palette.ground)
    }

    private var copyAddress: some View {
        Button {
            send(.copyAddress(orderId: orderId))
        } label: {
            Label(view.copyAddress.label, systemImage: "doc.on.doc")
                .frame(maxWidth: .infinity)
        }
        .glassButton()
        .disabled(!view.copyAddress.enabled)
    }
}

/// The payment code on its white card, with the badge that enlarges it;
/// faded once spent, so no one scans it again.
struct CodeCard: View {
    let code: String
    let said: String
    let spent: Bool
    var size: CGFloat = 240
    let enlarge: @MainActor () -> Void

    var body: some View {
        Button {
            enlarge()
        } label: {
            QRCodeView(text: code, label: said, size: size, padding: 16, cornerRadius: 24)
                .opacity(spent ? 0.25 : 1)
                .overlay(alignment: .bottomTrailing) {
                    Image(systemName: "arrow.up.left.and.arrow.down.right")
                        .font(.footnote.weight(.bold))
                        .foregroundStyle(Palette.codeInk)
                        .frame(width: 32, height: 32)
                        .background(Circle().fill(Palette.codeGround))
                        .overlay(Circle().stroke(Palette.border))
                        .offset(x: 8, y: 8)
                        .accessibilityHidden(true)
                }
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(said)
        .accessibilityHint("Enlarges the code")
        .accessibilityAddTraits(.isButton)
    }
}

/// How the payment stands: the loading mark while Till watches, the tick
/// once paid, the status and the hint under it.
struct SaleStatus: View {
    let status: String
    let hint: String
    let step: Int

    var body: some View {
        HStack(spacing: 12) {
            if step >= 4 {
                DoneTick(size: 28)
            } else {
                LoadingMark(size: 28, label: status)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(status)
                    .font(.body.weight(.semibold))
                Text(hint)
                    .font(.subheadline)
                    .foregroundStyle(Palette.secondary)
                    .monospacedDigit()
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 20)
        .accessibilityElement(children: .combine)
    }
}

/// The code enlarged for the customer (PadCode, PadFront; the design's
/// CodeB), drawn from the engine's `CodeView`: white whatever the theme,
/// the screen at full brightness while it shows, the amount, the line
/// under it, the code as large as the screen allows and the four steps.
/// A tap anywhere shrinks it. On iPhone (PhoneCode) the amount and the
/// shrink button head the screen, and the code takes its width.
struct CodeScreen: View {
    let view: CodeView
    let send: Send
    var phone = false

    @State private var brightness: CGFloat?

    var body: some View {
        Group {
            if phone {
                phoneLayout
            } else {
                panelLayout
            }
        }
        .foregroundStyle(Palette.codeInk)
        .background(Palette.codeGround)
        .contentShape(Rectangle())
        .onTapGesture { send(.back) }
        .accessibilityAction(named: "Shrink") { send(.back) }
        .onAppear { raiseBrightness() }
        .onDisappear { restoreBrightness() }
        .environment(\.colorScheme, .light)
    }

    /// PhoneCode: the amount and its line beside the shrink button, the
    /// code edge to edge, and what to do with it.
    private var phoneLayout: some View {
        GeometryReader { box in
            let side = max(120, min(box.size.width - 12, box.size.height - 190))
            VStack(spacing: 0) {
                HStack(alignment: .center, spacing: 8) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(view.amount)
                            .display(30, tracking: -0.5)
                            .minimumScaleFactor(0.5)
                            .lineLimit(1)
                        Text(view.under)
                            .font(.subheadline.monospacedDigit())
                            .foregroundStyle(Color(hex: 0x5A564C))
                            .lineLimit(1)
                    }
                    .accessibilityElement(children: .combine)
                    Spacer(minLength: 8)
                    Button {
                        send(.back)
                    } label: {
                        Image(systemName: "arrow.down.right.and.arrow.up.left")
                            .font(.body.weight(.semibold))
                            .frame(width: 48, height: 48)
                            .background(Circle().fill(Color(hex: 0xEFEAE0)))
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Shrink the code")
                }
                .padding(.leading, 14)
                .padding(.trailing, 10)
                .padding(.top, 6)
                Spacer(minLength: 8)
                QRCodeView(text: view.code, label: "Payment code for \(view.amount)", size: side, padding: 12, cornerRadius: 0)
                Spacer(minLength: 8)
                HStack(spacing: 10) {
                    LoadingMark(size: 26, label: "Waiting for payment")
                    Text("Scan with any Monero wallet")
                        .font(.headline)
                }
                Text("Tap anywhere to shrink")
                    .font(.footnote)
                    .foregroundStyle(Color(hex: 0x5A564C))
                    .padding(.top, 6)
                    .padding(.bottom, 8)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .padding(.horizontal, 6)
        }
    }

    /// PadCode and PadFront: the amount over the code, and the four steps.
    private var panelLayout: some View {
        GeometryReader { box in
            let side = max(120, min(box.size.width - 32, box.size.height - 260, 560))
            VStack(spacing: 14) {
                Spacer(minLength: 8)
                Text(view.amount)
                    .display(40, tracking: -1)
                    .minimumScaleFactor(0.5)
                    .lineLimit(1)
                Text(view.under)
                    .font(.subheadline.monospacedDigit())
                    .foregroundStyle(Color(hex: 0x5A564C))
                QRCodeView(text: view.code, label: "Payment code for \(view.amount)", size: side, padding: 12, cornerRadius: 0)
                StepBar(step: Int(view.step))
                    .frame(maxWidth: side)
                Text("Scan with any Monero wallet")
                    .font(.headline)
                Spacer(minLength: 8)
                Text("Tap anywhere to shrink")
                    .font(.footnote)
                    .foregroundStyle(Color(hex: 0x5A564C))
                    .padding(.bottom, 12)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .padding(.horizontal, 16)
        }
    }

    /// The screen this window is on.
    private var screen: UIScreen? {
        UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first?.screen
    }

    private func raiseBrightness() {
        guard let screen, brightness == nil else { return }
        brightness = screen.brightness
        screen.brightness = 1
    }

    private func restoreBrightness() {
        guard let screen, let saved = brightness else { return }
        screen.brightness = saved
        brightness = nil
    }
}

/// The four steps a payment goes through, as a bar: waiting, seen,
/// confirming, paid.
struct StepBar: View {
    let step: Int

    var body: some View {
        HStack(spacing: 6) {
            ForEach(1...4, id: \.self) { index in
                Capsule()
                    .fill(index <= step ? Palette.accent : Color(hex: 0xE4DFD5))
                    .frame(height: 6)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Step \(max(step, 1)) of 4")
    }
}

/// Every line of a sale's order (Screen.order, the design's PanelE),
/// opened from its card's "+N more": the count, each line, the subtotal,
/// tip, discount and taxes, and the total, as the engine's `OrderPageView`
/// writes them.
struct OrderScreen: View {
    /// The sale's title, under "The order" until its receipt names it.
    let title: String
    let order: OrderPageView?
    let send: Send

    var body: some View {
        VStack(spacing: 0) {
            TillBar(title: "The order", line: order?.subtitle ?? title, back: { send(.back) })
            ScrollView {
                if let order {
                    VStack(alignment: .leading, spacing: 16) {
                        HStack(alignment: .lastTextBaseline) {
                            Text(order.items)
                                .font(.largeTitle.weight(.bold))
                            Spacer(minLength: 8)
                            Text(order.lines)
                                .font(.subheadline)
                                .foregroundStyle(Palette.secondary)
                        }
                        .padding(.horizontal, 16)
                        .accessibilityElement(children: .combine)
                        Card {
                            VStack(alignment: .leading, spacing: 8) {
                                ForEach(Array(order.rows.enumerated()), id: \.offset) { _, row in
                                    HStack(alignment: .firstTextBaseline, spacing: 12) {
                                        Text(row.quantity)
                                            .monospacedDigit()
                                            .foregroundStyle(Palette.secondary)
                                        Text(row.description)
                                            .fixedSize(horizontal: false, vertical: true)
                                        Spacer(minLength: 8)
                                        Text(row.price)
                                            .monospacedDigit()
                                    }
                                    .font(.subheadline)
                                    .accessibilityElement(children: .combine)
                                }
                                Rectangle()
                                    .fill(Palette.hairline)
                                    .frame(height: 1)
                                ForEach(Array(order.sums.enumerated()), id: \.offset) { _, sum in
                                    HStack(alignment: .firstTextBaseline) {
                                        Text(sum.name)
                                        Spacer(minLength: 8)
                                        Text(sum.value)
                                            .monospacedDigit()
                                    }
                                    .font(.subheadline)
                                    .foregroundStyle(Palette.secondary)
                                    .accessibilityElement(children: .combine)
                                }
                                HStack(alignment: .firstTextBaseline) {
                                    Text("Total")
                                    Spacer(minLength: 8)
                                    Text(order.total)
                                        .monospacedDigit()
                                }
                                .font(.headline)
                                .accessibilityElement(children: .combine)
                            }
                            .padding(16)
                        }
                    }
                    .padding(.vertical, 12)
                }
            }
            .scrollBounceBehavior(.basedOnSize)
        }
        .foregroundStyle(Palette.ink)
        .background(Palette.ground)
    }
}
