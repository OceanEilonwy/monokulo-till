import SwiftUI
import TillCore
import UIKit

/// A live sale (round 4's F2, G2 and G3), drawn from the engine's
/// `SaleView`, over the whole screen at every width with the system's bar:
/// whom it's for and its line, Share and Close the sale (an X) while it
/// waits. The amount, then the code's card: the code (a button that opens
/// it full screen), a hairline, and the status beside its ring, as Android
/// draws it; then what was bought, and Copy address with Return to Square at
/// the foot. On a wide landscape window the code sits on the left and the
/// rest, centred beside it, on the right.
///
/// From the moment the payment is seen the code is gone (core.md decision
/// 170): a 56 pt ring leads with "Seen · $13.88", the payment's bar and the
/// order, so nothing can be shown or paid twice. A cash sale's Paid in cash
/// stays at the foot (decision 7).
///
/// Nothing is sized for a device: the code is the largest the window leaves
/// room for (``SaleLayout/codeSide``).
struct SaleScreen: View {
    let orderId: String
    let view: SaleView
    let send: Send
    /// Return to Square, when it's offered (``BackToSquare``).
    var returnToSquare: (@MainActor @Sendable () -> Void)?

    @Environment(\.horizontalSizeClass) private var sizeClass

    private var regular: Bool { sizeClass == .regular }

    var body: some View {
        GeometryReader { box in
            let wide = regular && box.size.width > box.size.height && box.size.width >= 900
            ScrollView {
                Group {
                    if wide {
                        wideContent(width: box.size.width, height: box.size.height)
                    } else {
                        content(width: box.size.width, height: box.size.height)
                    }
                }
                .padding(.horizontal, regular ? SaleLayout.margin : 16)
                .padding(.vertical, 12)
                .frame(maxWidth: .infinity, minHeight: box.size.height)
            }
            .scrollBounceBehavior(.basedOnSize)
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            foot
                .frame(maxWidth: readableColumn)
                .padding(.horizontal, 16)
                .padding(.top, 10)
                .padding(.bottom, regular ? 20 : 8)
                .frame(maxWidth: .infinity)
                .background(Palette.ground)
        }
        .foregroundStyle(Palette.ink)
        .background(Palette.ground)
        .navigationTitle(view.title)
        .navigationSubtitle(view.titleLine)
        .toolbarTitleDisplayMode(.inline)
        .toolbar {
            if view.showsCode {
                ToolbarItem(placement: .topBarTrailing) { shareLink }
                if view.closes {
                    ToolbarItem(placement: .topBarTrailing) {
                        // An X, as on Android; the words are its label.
                        Button(role: .destructive) {
                            send(.askClose(orderId: orderId))
                        } label: {
                            Image(systemName: "xmark")
                                .font(.body.weight(.semibold))
                                .foregroundStyle(Palette.danger)
                        }
                        .accessibilityLabel(view.closeLabel)
                        .tint(Palette.danger)
                    }
                }
            }
        }
    }

    /// Copy address and Return to Square side by side, each on one line, then
    /// a cash sale's Paid in cash.
    @ViewBuilder
    private var foot: some View {
        VStack(spacing: 8) {
            HStack(spacing: 10) {
                if view.showsCode {
                    Button {
                        send(.copyAddress(orderId: orderId))
                    } label: {
                        Label(view.copyAddress.label, systemImage: "doc.on.doc")
                            .lineLimit(1)
                    }
                    .buttonStyle(.secondary)
                    .disabled(!view.copyAddress.enabled)
                }
                if let returnToSquare, let back = view.backToPos {
                    ReturnToSquareWide(label: back.press.label, action: returnToSquare)
                }
            }
            ForEach(Array(view.cash.enumerated()), id: \.offset) { _, button in
                LinkPress(press: button.press) { send(CashButtons.action(button.action, orderId: orderId)) }
            }
        }
    }

    /// Share the whole code as a picture, with the `monero:` payment link
    /// as its message (decisions 17 and 130); the link alone if the picture
    /// can't be made. Only while the code shows.
    @ViewBuilder
    private var shareLink: some View {
        let uri = SaleShare.uri(code: view.code, address: view.address)
        Group {
            if let picture = QRImages.shareImage(view.code) {
                let image = Image(uiImage: picture)
                ShareLink(
                    item: image,
                    subject: Text(view.shareTitle),
                    message: Text(uri),
                    preview: SharePreview(view.shareTitle, image: image)
                ) {
                    Image(systemName: "square.and.arrow.up")
                }
            } else {
                ShareLink(
                    item: uri,
                    subject: Text(view.shareTitle),
                    preview: SharePreview(view.shareTitle)
                ) {
                    Image(systemName: "square.and.arrow.up")
                }
            }
        }
        .disabled(!view.copyAddress.enabled)
        .accessibilityLabel(view.share)
    }

    /// One column: the amount, the code's card (or, once the payment is
    /// seen, the ring), and the order.
    private func content(width: CGFloat, height: CGFloat) -> some View {
        let side = SaleLayout.codeSide(width: min(width, readableColumn + SaleLayout.margin * 2), height: height, regular: regular)
        return VStack(spacing: 14) {
            if view.showsCode {
                amount
                codeCard(side: side)
            } else {
                statusHero
            }
            orderSection
        }
        .frame(maxWidth: readableColumn)
    }

    /// A wide landscape window (G3): the code's card on the left; the
    /// amount and the order beside it, centred against it.
    private func wideContent(width: CGFloat, height: CGFloat) -> some View {
        let side = max(240, min(460, height - 120))
        return HStack(alignment: .center, spacing: 48) {
            if view.showsCode {
                codeCard(side: side)
            } else {
                statusHero
                    .frame(width: side)
            }
            VStack(spacing: 16) {
                if view.showsCode {
                    amount
                }
                orderSection
            }
            .frame(maxWidth: 470)
        }
        .frame(maxWidth: .infinity, minHeight: height - 40)
    }

    private var amount: some View {
        VStack(spacing: 2) {
            Text(view.amount)
                .display(44, tracking: -1)
                .minimumScaleFactor(0.5)
                .lineLimit(1)
            Text(view.xmr)
                .font(.subheadline.monospacedDigit())
                .foregroundStyle(Palette.secondary)
                .lineLimit(1)
            if let priceLine = view.priceLine {
                Text(priceLine)
                    .font(.footnote.monospacedDigit())
                    .foregroundStyle(Palette.secondary)
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
            }
        }
        .frame(maxWidth: .infinity)
    }

    /// The code on its white card, which opens it full screen, a hairline,
    /// and the status left-aligned beside its ring (round 4).
    private func codeCard(side: CGFloat) -> some View {
        VStack(spacing: 0) {
            CodeCard(code: view.code, said: view.codeSaid, spent: view.spent, size: side) {
                send(.enlarge(orderId: orderId))
            }
            Rectangle()
                .fill(Palette.hairline)
                .frame(height: 1)
                .accessibilityHidden(true)
            HStack(spacing: 10) {
                StatusGlyph(kind: StatusGlyphKind.of(.ring, step: Int(view.step)), size: 22)
                VStack(alignment: .leading, spacing: 1) {
                    Text(view.status)
                        .font(.subheadline.weight(.semibold))
                        .lineLimit(1)
                    if !view.hint.isEmpty {
                        Text(view.hint)
                            .font(.footnote.monospacedDigit())
                            .foregroundStyle(Palette.secondary)
                            .lineLimit(1)
                    }
                }
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            .accessibilityElement(children: .combine)
            .accessibilityAddTraits(.updatesFrequently)
        }
        // The code's own card is `side` across; the status shares its white.
        .frame(width: side)
        .background(RoundedRectangle(cornerRadius: 20).fill(Color.white))
        .environment(\.colorScheme, .light)
    }

    /// Seen and Confirming: the ring at 56 pt, "Seen · $13.88", the XMR and
    /// the payment's bar. No code, no Share.
    private var statusHero: some View {
        VStack(spacing: 10) {
            StatusGlyph(kind: StatusGlyphKind.of(.ring, step: Int(view.step)), size: 56)
            Text(view.heroTitle)
                .display(28, tracking: -0.6)
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
        .padding(.top, 16)
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
}

/// Return to Square at the foot of a sale (round 4): silver, the Square
/// mark, its words on one line however narrow, as wide as its share of the
/// row.
struct ReturnToSquareWide: View {
    let label: String
    let action: @MainActor () -> Void

    var body: some View {
        Button {
            action()
        } label: {
            HStack(spacing: 8) {
                SquareMark()
                    .frame(width: 18, height: 18)
                Text(label)
                    .font(.body.weight(.semibold))
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
            }
            .foregroundStyle(Color.black)
            .frame(maxWidth: .infinity, minHeight: 50)
            .background(Capsule().fill(Color(hex: 0xD6D9DE)))
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
        .accessibilityHint("Opens Square Point of Sale")
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
                        if let priceLine = view.priceLine {
                            Text(priceLine)
                                .font(.footnote.monospacedDigit())
                                .foregroundStyle(Color(hex: 0x5A564C))
                                .lineLimit(1)
                        }
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
                if let priceLine = view.priceLine {
                    Text(priceLine)
                        .font(.footnote.monospacedDigit())
                        .foregroundStyle(Color(hex: 0x5A564C))
                        .lineLimit(1)
                }
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
