import SwiftUI
import TillCore
import UIKit

/// A live sale (PadSale, PhoneSale; the design's SaleBody), drawn from the
/// engine's `SaleView`: the amount and the XMR (tapped, it's copied), the
/// code with its enlarge badge, how the payment stands, what was bought,
/// and Copy address. The bar's tinted X asks whether the customer is
/// paying. iOS can't open Square (docs/ios-plan.md §4), so there's no
/// "Back to Square".
struct SaleScreen: View {
    let orderId: String
    let view: SaleView
    let send: Send

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
                    CodeCard(code: view.code, said: view.codeSaid, spent: view.spent) {
                        send(.enlarge(orderId: orderId))
                    }
                    SaleStatus(status: view.status, hint: view.hint, step: Int(view.step))
                    if let order = view.order {
                        OrderCardSection(card: order) { send(.showOrder(orderId: orderId)) }
                    }
                    Button {
                        send(.copyAddress(orderId: orderId))
                    } label: {
                        Label(view.copyAddress.label, systemImage: "doc.on.doc")
                            .frame(maxWidth: .infinity)
                    }
                    .glassButton()
                    .disabled(!view.copyAddress.enabled)
                    .padding(.horizontal, 16)
                }
                .padding(.bottom, 24)
            }
            .scrollBounceBehavior(.basedOnSize)
        }
        .foregroundStyle(Palette.ink)
        .background(Palette.ground)
    }
}

/// The payment code on its white card, with the badge that enlarges it;
/// faded once spent, so no one scans it again.
struct CodeCard: View {
    let code: String
    let said: String
    let spent: Bool
    let enlarge: @MainActor () -> Void

    var body: some View {
        Button {
            enlarge()
        } label: {
            QRCodeView(text: code, label: said, size: 240, padding: 16, cornerRadius: 24)
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
/// A tap anywhere shrinks it.
struct CodeScreen: View {
    let view: CodeView
    let send: Send

    @State private var brightness: CGFloat?

    var body: some View {
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
        .foregroundStyle(Palette.codeInk)
        .background(Palette.codeGround)
        .contentShape(Rectangle())
        .onTapGesture { send(.back) }
        .accessibilityAction(named: "Shrink") { send(.back) }
        .onAppear { raiseBrightness() }
        .onDisappear { restoreBrightness() }
        .environment(\.colorScheme, .light)
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

/// Every line of a sale's order (Screen.order), opened from its card's
/// "+N more".
struct OrderScreen: View {
    let title: String
    let line: String
    let order: OrderCard?
    let send: Send

    var body: some View {
        VStack(spacing: 0) {
            TillBar(title: title, line: line, back: { send(.back) })
            ScrollView {
                if let order {
                    OrderCardSection(card: order)
                        .padding(.vertical, 12)
                }
            }
            .scrollBounceBehavior(.basedOnSize)
        }
        .foregroundStyle(Palette.ink)
        .background(Palette.ground)
    }
}
