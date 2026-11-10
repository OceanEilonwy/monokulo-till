import SwiftUI
import TillCore
import UIKit

/// A live sale (D1's detail, D2), drawn from the engine's `SaleView`.
///
/// Regular width (D1, the DesignPad board): a header row under the
/// system's bar with whom it's for (28 pt, decision 10), its line, and
/// Close the sale as a tinted capsule at the end; then, centred in what's
/// left, the amount, the XMR, the code on its white card and the payment's
/// progress, with what was bought beside them where both fit and under
/// them where they don't; at the foot, Customer view (the full-screen
/// code, to turn towards the customer) and Share, which shares the
/// `monero:` payment link (decision 17), and a cash sale's Paid in cash
/// (decision 7).
///
/// Compact width (D2, a narrow iPad window): the title and Close in the
/// system's bar, one column, the buttons stacked.
///
/// Nothing is sized for a device: the column, the code and the buttons
/// follow the space the window gives them (decisions 27–31).
struct SaleScreen: View {
    let orderId: String
    let view: SaleView
    let send: Send

    @Environment(\.horizontalSizeClass) private var sizeClass

    private var regular: Bool { sizeClass == .regular }

    var body: some View {
        // Inside the safe area, and inside the header and the buttons, so
        // the column centres in what's left and scrolls only when it must.
        GeometryReader { box in
            ScrollView {
                content(width: box.size.width, height: box.size.height)
                    .padding(.horizontal, regular ? SaleLayout.margin : 0)
                    .padding(.vertical, 16)
                    .frame(maxWidth: .infinity, minHeight: box.size.height)
            }
            .scrollBounceBehavior(.basedOnSize)
        }
        .safeAreaInset(edge: .top, spacing: 0) {
            if regular {
                header
                    .padding(.horizontal, SaleLayout.margin)
                    .padding(.top, 4)
                    .padding(.bottom, 8)
                    .background(Palette.ground)
            }
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            actions
                .padding(.horizontal, 20)
                .padding(.top, 12)
                .padding(.bottom, regular ? 24 : 16)
                .background(Palette.ground)
        }
        .foregroundStyle(Palette.ink)
        .background(Palette.ground)
        .navigationTitle(view.title)
        .navigationSubtitle(view.titleLine)
        .toolbarTitleDisplayMode(.inline)
        // On iPad the header row carries the title; the bar keeps only
        // what the system puts there (the sidebar button).
        .toolbar(removing: regular ? .title : nil)
        .toolbar {
            if view.closes && !regular {
                ToolbarItem(placement: .topBarTrailing) {
                    Button(role: .destructive) {
                        send(.askClose(orderId: orderId))
                    } label: {
                        Label(view.closeLabel, systemImage: "xmark")
                    }
                    .tint(Palette.danger)
                }
            }
        }
    }

    /// D1's header: the name and its line, one line each, and Close the
    /// sale, the system's bordered capsule in the danger tint.
    private var header: some View {
        HStack(alignment: .center, spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(view.title)
                    .font(.title.weight(.bold))
                    .lineLimit(1)
                    .accessibilityAddTraits(.isHeader)
                Text(view.titleLine)
                    .font(.footnote)
                    .foregroundStyle(Palette.secondary)
                    .monospacedDigit()
                    .lineLimit(1)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if view.closes {
                Button(role: .destructive) {
                    send(.askClose(orderId: orderId))
                } label: {
                    Text(view.closeLabel)
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(Palette.danger)
                        .lineLimit(1)
                }
                .buttonStyle(.bordered)
                .buttonBorderShape(.capsule)
                .controlSize(.large)
                .tint(Palette.danger)
                .fixedSize()
            }
        }
        .frame(minHeight: minimumTarget)
    }

    /// The code column, and the order beside it where both fit, else under
    /// it; on a phone-width window, one column.
    @ViewBuilder
    private func content(width: CGFloat, height: CGFloat) -> some View {
        let side = SaleLayout.codeSide(width: width, height: regular ? height : .infinity, regular: regular)
        if regular {
            ViewThatFits(in: .horizontal) {
                HStack(alignment: .center, spacing: SaleLayout.gap) {
                    codeColumn(side: side)
                    if let order = view.order {
                        orderColumn(order)
                            .frame(minWidth: 200, idealWidth: 240, maxWidth: 280)
                    }
                }
                VStack(spacing: 24) {
                    codeColumn(side: side)
                    if let order = view.order {
                        orderColumn(order)
                            .frame(maxWidth: side + 28)
                    }
                }
            }
        } else {
            VStack(spacing: 16) {
                codeColumn(side: side)
                if let order = view.order {
                    OrderCardSection(card: order) { send(.showOrder(orderId: orderId)) }
                }
            }
            .frame(maxWidth: readableColumn)
        }
    }

    /// "ORDER" over the receipt's card.
    private func orderColumn(_ order: OrderCard) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            SectionCaption(text: view.orderTitle)
            OrderCardSection(card: order) { send(.showOrder(orderId: orderId)) }
                .padding(.horizontal, -16)
        }
    }

    /// The amount, the code and the progress, as one column as wide as
    /// the code's card.
    private func codeColumn(side: CGFloat) -> some View {
        VStack(spacing: 8) {
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
                .padding(.top, 10)
        }
        .frame(width: side + 28)
    }

    /// The payment's bar and words: the engine's thirds where it gives
    /// them, else the ring's step and status.
    @ViewBuilder
    private var progress: some View {
        if let progress = view.progress {
            PaymentProgress(
                step: Int(view.step),
                status: progress.label.isEmpty ? progress.line : progress.label,
                count: EngineProgress.count(label: progress.label, count: progress.count, line: progress.line),
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
            if regular {
                // Side by side at their own widths where they fit (the
                // board's row), else stacked in a column.
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 12) {
                        customerView.fixedSize(horizontal: true, vertical: false)
                        share.fixedSize(horizontal: true, vertical: false)
                    }
                    VStack(spacing: 10) {
                        customerView
                        share
                    }
                    .frame(maxWidth: SaleLayout.stackedButtons)
                }
                .frame(maxWidth: .infinity)
            } else {
                VStack(spacing: 10) {
                    customerView
                    share
                }
            }
            ForEach(Array(view.cash.enumerated()), id: \.offset) { _, button in
                LinkPress(press: button.press) { send(CashButtons.action(button.action, orderId: orderId)) }
                    .frame(maxWidth: regular ? SaleLayout.stackedButtons : .infinity)
            }
        }
    }

    private var customerView: some View {
        Button {
            send(.enlarge(orderId: orderId))
        } label: {
            Label(view.customerView, systemImage: "arrow.2.circlepath")
                .lineLimit(1)
                .frame(minWidth: regular ? 180 : nil)
        }
        .buttonStyle(.primary)
        .accessibilityHint("Shows the code full screen, to turn towards the customer")
    }

    private var share: some View {
        ShareLink(
            item: SaleShare.uri(code: view.code, address: view.address),
            subject: Text(view.amount),
            preview: SharePreview(view.shareTitle)
        ) {
            Label(view.share, systemImage: "square.and.arrow.up")
                .lineLimit(1)
                .frame(minWidth: regular ? 100 : nil)
        }
        .buttonStyle(.secondary)
        .disabled(!view.copyAddress.enabled)
    }
}

/// The live sale's measures on iPad (the DesignPad board), and how the
/// code shrinks to the window rather than overflowing it.
enum SaleLayout {
    /// The page's side margin on iPad.
    static let margin: CGFloat = 28
    /// Between the code's column and the order's.
    static let gap: CGFloat = 40
    /// The widest the buttons grow once stacked.
    static let stackedButtons: CGFloat = 360

    /// What the code's column needs besides the code itself: the amount,
    /// the XMR, the card's padding, the progress and its lines, and the
    /// page's padding above and below.
    static let besideCode: CGFloat = 240

    /// The code's side: the board's 300 pt on iPad and 240 pt on a phone,
    /// smaller where the window is narrower than the code's card (its 14 pt
    /// padding each side) and the page's margins, or too short for the
    /// whole column (a short Stage Manager window), never under 160 pt.
    static func codeSide(width: CGFloat, height: CGFloat = .infinity, regular: Bool) -> CGFloat {
        let largest: CGFloat = regular ? 300 : 240
        let margins: CGFloat = regular ? margin * 2 : 32
        return max(160, min(largest, width - margins - 28, height - besideCode))
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
