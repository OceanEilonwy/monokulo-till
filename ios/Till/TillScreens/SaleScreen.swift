import SwiftUI
import TillCore
import UIKit

/// A live sale (the iPad round 3 designs, Q0 to Q8), drawn from the
/// engine's `SaleView`: one vertical column under a header row (the system's
/// bar on a narrow window) that carries whom it's for and its line, and at
/// the end Share, Return to Square and Close the sale while it waits (Return
/// to Square and Done once the payment is seen). The column is the amount,
/// the XMR, the code on its white card (a button that opens it full screen
/// with the amount, so there is no Customer view button), and what was
/// bought: a long order folds to one row that opens every line.
///
/// From the moment the payment is seen the code is gone (core.md decision
/// 170): the Till logo leads with "Seen · $13.88", the payment's bar and the
/// order, so nothing can be shown or paid twice. A cash sale's Paid in cash
/// stays at the foot (decision 7).
///
/// Nothing is sized for a device: the code is the largest the window leaves
/// room for with the order folded (``SaleLayout/codeSide``), so the page
/// never scrolls however long the receipt is.
struct SaleScreen: View {
    let orderId: String
    let view: SaleView
    let send: Send
    /// Return to Square, when it's offered (``BackToSquare``).
    var returnToSquare: (@MainActor @Sendable () -> Void)?

    @Environment(\.horizontalSizeClass) private var sizeClass

    private var regular: Bool { sizeClass == .regular }

    var body: some View {
        // Inside the safe area, and inside the header and the cash button,
        // so the column centres in what's left and scrolls only when it must.
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
            if !view.cash.isEmpty {
                cashActions
                    .padding(.horizontal, 20)
                    .padding(.top, 12)
                    .padding(.bottom, regular ? 24 : 16)
                    .background(Palette.ground)
            }
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
            if !regular {
                if view.showsCode {
                    ToolbarItem(placement: .topBarTrailing) { shareLink(labelled: false) }
                }
                if let returnToSquare, let back = view.backToPos {
                    ToolbarItem(placement: .topBarTrailing) {
                        ReturnToSquareButton(label: back.press.label, compact: true, action: returnToSquare)
                    }
                }
                if view.showsCode {
                    if view.closes {
                        ToolbarItem(placement: .topBarTrailing) {
                            Button(role: .destructive) {
                                send(.askClose(orderId: orderId))
                            } label: {
                                Label(view.closeLabel, systemImage: "xmark")
                            }
                            .tint(Palette.danger)
                        }
                    }
                } else {
                    ToolbarItem(placement: .confirmationAction) { done }
                }
            }
        }
    }

    /// The header row on iPad: the name and its line, one line each, then
    /// the buttons (the designs' toolbar group): Share, Return to Square and
    /// Close the sale while it waits, Return to Square and Done after.
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
            HStack(spacing: 8) {
                if view.showsCode {
                    shareLink(labelled: true)
                }
                if let returnToSquare, let back = view.backToPos {
                    ReturnToSquareButton(label: back.press.label, compact: false, action: returnToSquare)
                }
                if view.showsCode {
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
                } else {
                    done
                }
            }
        }
        .frame(minHeight: minimumTarget)
    }

    private var done: some View {
        Button("Done") { send(.back) }
            .buttonStyle(.bordered)
            .buttonBorderShape(.capsule)
            .controlSize(.large)
            .tint(Palette.link)
    }

    /// Share the `monero:` payment link (decision 17).
    private func shareLink(labelled: Bool) -> some View {
        ShareLink(
            item: SaleShare.uri(code: view.code, address: view.address),
            subject: Text(view.amount),
            preview: SharePreview(view.shareTitle)
        ) {
            if labelled {
                Label(view.share, systemImage: "square.and.arrow.up")
                    .font(.subheadline.weight(.semibold))
                    .lineLimit(1)
            } else {
                Image(systemName: "square.and.arrow.up")
            }
        }
        .buttonStyle(.bordered)
        .buttonBorderShape(.capsule)
        .controlSize(labelled ? .large : .regular)
        .tint(Palette.ink)
        .disabled(!view.copyAddress.enabled)
        .accessibilityLabel(view.share)
    }

    /// One vertical column: the code (or, once the payment is seen, the
    /// logo) and the order.
    private func content(width: CGFloat, height: CGFloat) -> some View {
        let side = SaleLayout.codeSide(width: width, height: height, regular: regular)
        return VStack(spacing: 12) {
            if view.showsCode {
                codeHero(side: side)
            } else {
                statusHero
            }
            orderSection
        }
        .frame(maxWidth: readableColumn)
    }

    /// While it waits: the amount, the code that opens full screen when it
    /// is tapped (its badge and a line say so), and the hint. The bar comes
    /// from Seen on: there is nothing to confirm yet.
    private func codeHero(side: CGFloat) -> some View {
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
            .padding(.top, 14)
            Text(view.enlargeHint)
                .font(.footnote)
                .foregroundStyle(Palette.secondary)
                .lineLimit(1)
            if view.step >= 2 {
                progress
                    .frame(width: side + 28)
                    .padding(.top, 6)
            } else if !view.hint.isEmpty {
                Text(view.hint)
                    .font(.subheadline.monospacedDigit())
                    .foregroundStyle(Palette.secondary)
                    .lineLimit(1)
                    .padding(.top, 2)
            }
        }
        .frame(maxWidth: .infinity)
    }

    /// Seen and Confirming: the Till logo, "Seen · $13.88", the XMR and the
    /// payment's bar. No code, no Share.
    private var statusHero: some View {
        VStack(spacing: 10) {
            TillWidget(state: view.step >= 3 ? .confirming : .seen, size: regular ? 160 : 120)
            Text(view.heroTitle)
                .display(34, tracking: -0.8)
                .multilineTextAlignment(.center)
                .accessibilityAddTraits(.isHeader)
            Text(view.xmr)
                .font(.subheadline.monospacedDigit())
                .foregroundStyle(Palette.secondary)
                .lineLimit(1)
            progress
                .frame(maxWidth: 340)
                .padding(.top, 4)
        }
        .frame(maxWidth: .infinity)
        .padding(.horizontal, 20)
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

    /// What was bought: all of a short order, one folded row for a long one
    /// that opens every line.
    @ViewBuilder
    private var orderSection: some View {
        if let order = view.order {
            if let folded = order.folded {
                FoldedOrderRow(folded: folded) { send(.showOrder(orderId: orderId)) }
            } else {
                OrderCardSection(card: order) { send(.showOrder(orderId: orderId)) }
            }
        }
    }

    /// A cash sale's Paid in cash.
    private var cashActions: some View {
        VStack(spacing: 6) {
            ForEach(Array(view.cash.enumerated()), id: \.offset) { _, button in
                LinkPress(press: button.press) { send(CashButtons.action(button.action, orderId: orderId)) }
                    .frame(maxWidth: regular ? SaleLayout.stackedButtons : .infinity)
            }
        }
    }
}

/// The live sale's measures, and how the code is capped so the page never
/// scrolls with the order folded (the iPad round 3 designs).
enum SaleLayout {
    /// The page's side margin on iPad.
    static let margin: CGFloat = 28
    /// The widest the cash button grows.
    static let stackedButtons: CGFloat = 360

    /// What the column needs besides the code: the amount, the XMR, the
    /// card's padding, the hint line, the folded order row and the page's
    /// spacing.
    static let besideCode: CGFloat = 260

    /// The code's side: the largest the window leaves room for, in width
    /// (less the page's margins and the card's padding) and in height (less
    /// everything else), 480 pt at most, and never under 200 pt: smaller than
    /// that it can't be scanned, and tapping it for the full-screen code is
    /// the way in.
    static func codeSide(width: CGFloat, height: CGFloat = .infinity, regular: Bool) -> CGFloat {
        let margins: CGFloat = regular ? margin * 2 : 32
        return max(200, min(480, width - margins - 28, height - besideCode))
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
                .overlay(alignment: .bottomTrailing) {
                    // It opens full screen: the badge says so.
                    Image(systemName: "arrow.up.left.and.arrow.down.right")
                        .font(.caption.weight(.bold))
                        .foregroundStyle(.white)
                        .frame(width: 30, height: 30)
                        .background(Circle().fill(Color.black.opacity(0.78)))
                        .padding(8)
                        .accessibilityHidden(true)
                }
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

    /// Done at the top right (the design's Q0b and Q8b), which brings the
    /// code back down to its sale.
    private var shrink: some View {
        Button {
            send(.back)
        } label: {
            Text("Done")
                .font(.body.weight(.semibold))
                .padding(.horizontal, 20)
                .frame(minHeight: 44)
                .background(Capsule().fill(Color(hex: 0xEFEAE0)))
        }
        .buttonStyle(.plain)
        .accessibilityHint("Shrinks the code back to its sale")
    }

    /// PhoneCode: the amount and its line beside Done, the
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
