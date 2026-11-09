import SwiftUI
import TillCore
import UIKit

/// A live sale (D1's detail, D2), drawn from the engine's `SaleView`: whom
/// it's for and its line as the toolbar's title, Close in the toolbar, the
/// amount and the XMR, the code on its white card, the payment's progress
/// bar and its label, what was bought, and at the foot Customer view (the
/// full-screen code, to turn towards the customer) and Share, which shares
/// the `monero:` payment link (decision 17), and a cash sale's Paid in cash
/// (decision 7). The customer's name is the title, 28 pt on iPad
/// (decision 10).
struct SaleScreen: View {
    let orderId: String
    let view: SaleView
    let send: Send

    @Environment(\.horizontalSizeClass) private var sizeClass

    private var regular: Bool { sizeClass == .regular }

    var body: some View {
        ScrollView {
            Group {
                if regular, let order = view.order {
                    HStack(alignment: .center, spacing: 40) {
                        codeColumn
                        VStack(alignment: .leading, spacing: 10) {
                            SectionCaption(text: view.orderTitle)
                            OrderCardSection(card: order) { send(.showOrder(orderId: orderId)) }
                                .padding(.horizontal, -16)
                        }
                        .frame(width: 260)
                    }
                    .frame(maxWidth: .infinity)
                } else {
                    VStack(spacing: 16) {
                        codeColumn
                        if let order = view.order {
                            OrderCardSection(card: order) { send(.showOrder(orderId: orderId)) }
                        }
                    }
                }
            }
            .padding(.vertical, 16)
        }
        .readableWidth(regular ? 760 : readableColumn)
        .safeAreaInset(edge: .bottom) {
            actions
                .padding(.horizontal, 20)
                .padding(.top, 8)
                .padding(.bottom, 8)
                .background(Palette.ground)
        }
        .foregroundStyle(Palette.ink)
        .background(Palette.ground)
        .navigationTitle(view.title)
        .navigationSubtitle(view.titleLine)
        .toolbarTitleDisplayMode(regular ? .inlineLarge : .inline)
        .toolbar {
            if view.closes {
                ToolbarItem(placement: .topBarTrailing) {
                    Button(role: .destructive) {
                        send(.askClose(orderId: orderId))
                    } label: {
                        if regular {
                            Text(view.closeLabel)
                        } else {
                            Label(view.closeLabel, systemImage: "xmark")
                        }
                    }
                    .tint(Palette.danger)
                }
            }
        }
    }

    /// The amount, the code and the progress, as one column.
    private var codeColumn: some View {
        let side: CGFloat = regular ? 300 : 240
        return VStack(spacing: 8) {
            Text(view.amount)
                .display(44, tracking: -1)
                .minimumScaleFactor(0.5)
                .lineLimit(1)
            Text(view.xmr)
                .font(.subheadline.monospacedDigit())
                .foregroundStyle(Palette.secondary)
                .lineLimit(1)
            CodeCard(code: view.code, said: view.codeSaid, spent: view.spent, size: side) {
                send(.enlarge(orderId: orderId))
            }
            .padding(.top, 4)
            progress
                .frame(width: side + 28)
                .padding(.top, 6)
        }
        .frame(maxWidth: .infinity)
    }

    /// The payment's bar and words: the engine's thirds where it gives
    /// them, else the ring's step and status.
    @ViewBuilder
    private var progress: some View {
        if let progress = view.progress {
            PaymentProgress(
                step: Int(view.step),
                status: progress.line,
                hint: view.hint,
                fraction: EngineProgress.fraction(value: progress.value, total: progress.total),
                said: progress.said
            )
        } else {
            PaymentProgress(step: Int(view.step), status: view.status, hint: view.hint)
        }
    }

    /// Customer view, the primary, and Share; under them a cash sale's
    /// Paid in cash.
    private var actions: some View {
        VStack(spacing: 6) {
            mainActions
            ForEach(Array(view.cash.enumerated()), id: \.offset) { _, button in
                LinkPress(press: button.press) { send(CashButtons.action(button.action, orderId: orderId)) }
                    .frame(maxWidth: regular ? 452 : .infinity)
            }
        }
    }

    @ViewBuilder
    private var mainActions: some View {
        let layout = regular
            ? AnyLayout(HStackLayout(spacing: 12))
            : AnyLayout(VStackLayout(spacing: 10))
        layout {
            Button {
                send(.enlarge(orderId: orderId))
            } label: {
                Label(view.customerView, systemImage: "arrow.2.circlepath")
            }
            .buttonStyle(.primary)
            .frame(maxWidth: regular ? 260 : .infinity)
            .accessibilityHint("Shows the code full screen, to turn towards the customer")
            ShareLink(
                item: SaleShare.uri(code: view.code, address: view.address),
                subject: Text(view.amount),
                preview: SharePreview(view.shareTitle)
            ) {
                Label(view.share, systemImage: "square.and.arrow.up")
            }
            .buttonStyle(.secondary)
            .frame(maxWidth: regular ? 180 : .infinity)
            .disabled(!view.copyAddress.enabled)
        }
        .frame(maxWidth: .infinity)
    }
}

/// What Share sends: the sale's `monero:` payment link (the code's own
/// text), which gives Copy, AirDrop and Messages.
enum SaleShare {
    static func uri(code: String, address: String) -> String {
        code.lowercased().hasPrefix("monero:") ? code : "monero:\(address)"
    }
}

/// Small capitals over a column: "ORDER".
struct SectionCaption: View {
    let text: String

    var body: some View {
        Text(text.uppercased())
            .font(.footnote.weight(.semibold))
            .foregroundStyle(Palette.secondary)
            .accessibilityAddTraits(.isHeader)
    }
}

/// The payment code on its plain white card, which also opens the
/// customer's view; faded once spent, so no one scans it again.
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
            QRCodeView(text: code, label: said, size: size, padding: 14, cornerRadius: 20)
                .opacity(spent ? 0.25 : 1)
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(said)
        .accessibilityHint("Shows the code full screen")
        .accessibilityAddTraits(.isButton)
    }
}

/// The customer's view of the code (PadCode, PhoneCode), drawn from the
/// engine's `CodeView`: white whatever the theme, the screen at full
/// brightness while it shows, the amount and the line under it, the code
/// as large as the screen allows, and the payment's progress. The shrink
/// button, never a tap anywhere, gives it back to the cashier (customers
/// touch the screen they scan).
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
        .accessibilityAction(named: "Shrink") { send(.back) }
        .onAppear { raiseBrightness() }
        .onDisappear { restoreBrightness() }
        .environment(\.colorScheme, .light)
    }

    private var shrink: some View {
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

    /// PhoneCode: the amount and its line beside the shrink button, the
    /// code edge to edge, and how the payment stands.
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
                    shrink
                }
                .padding(.leading, 14)
                .padding(.trailing, 10)
                .padding(.top, 6)
                Spacer(minLength: 8)
                QRCodeView(text: view.code, label: view.codeSaid, size: side, padding: 12, cornerRadius: 0)
                Spacer(minLength: 8)
                PaymentProgress(step: Int(view.step), status: view.scan)
                    .frame(maxWidth: min(side, 320))
                    .padding(.bottom, 12)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .padding(.horizontal, 6)
        }
    }

    /// PadCode: the amount over the code, and how the payment stands.
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
                    .lineLimit(1)
                QRCodeView(text: view.code, label: view.codeSaid, size: side, padding: 12, cornerRadius: 0)
                PaymentProgress(step: Int(view.step), status: view.scan)
                    .frame(maxWidth: side)
                Spacer(minLength: 8)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .padding(.horizontal, 16)
            .overlay(alignment: .topTrailing) {
                shrink.padding(16)
            }
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
        ScrollView {
            if let order {
                VStack(alignment: .leading, spacing: 16) {
                    HStack(alignment: .lastTextBaseline) {
                        Text(order.items)
                            .font(.title.weight(.bold))
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
                                        .fixedSize()
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
                                        .fixedSize()
                                }
                                .font(.subheadline)
                                .foregroundStyle(Palette.secondary)
                                .accessibilityElement(children: .combine)
                            }
                            HStack(alignment: .firstTextBaseline) {
                                Text(order.totalLabel)
                                Spacer(minLength: 8)
                                Text(order.total)
                                    .monospacedDigit()
                                    .fixedSize()
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
        .readableWidth()
        .foregroundStyle(Palette.ink)
        .background(Palette.ground)
        .navigationTitle(order?.title ?? "")
        .navigationSubtitle(order?.subtitle ?? title)
        .navigationBarTitleDisplayMode(.inline)
    }
}
