import SwiftUI
import TillCore

/// The close sheet, "Is the customer paying?" (PadClose, PhoneClose; the
/// design's CloseAsk), drawn from the engine's `CloseView`: the question,
/// and its answers, each with the line under it.
struct CloseSheetScreen: View {
    let sheet: CloseSheet
    let send: Send

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: 6) {
                Heading(text: sheet.view.question, size: 24)
                Lead(text: sheet.view.subtitle)
            }
            .padding(.horizontal, 20)
            .padding(.top, 24)
            .padding(.bottom, 16)
            Card {
                ForEach(Array(sheet.view.answers.enumerated()), id: \.offset) { index, answer in
                    if index > 0 {
                        Hairline()
                    }
                    Button {
                        send(.close(orderId: sheet.orderId, choice: answer.choice))
                    } label: {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(answer.title)
                                .font(.body.weight(.semibold))
                                .foregroundStyle(CloseSheetScreen.ink(answer.choice))
                            Text(answer.detail)
                                .font(.subheadline)
                                .foregroundStyle(Palette.secondary)
                                .monospacedDigit()
                                .fixedSize(horizontal: false, vertical: true)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 16)
                        .padding(.vertical, 12)
                        .frame(minHeight: 60)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityElement(children: .combine)
                }
            }
            Spacer(minLength: 16)
        }
        .foregroundStyle(Palette.ink)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(Palette.ground)
    }

    /// An answer's words: refunding in danger, the rest in ink.
    static func ink(_ choice: CloseChoice) -> Color {
        switch choice {
        case .refundIt: Palette.danger
        default: Palette.ink
        }
    }
}

/// The refund sheet (decision 130 of android.md), drawn from the engine's
/// `RefundSheetView`: what to refund and where, "I've refunded it" and
/// "Not yet".
struct RefundSheetScreen: View {
    let view: RefundSheetView
    let send: Send

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Heading(text: view.title, size: 24)
            Rich.text(view.body)
                .font(.body)
                .foregroundStyle(Palette.secondary)
                .fixedSize(horizontal: false, vertical: true)
            if let notSeen = view.notSeen {
                NoteCard(text: notSeen, ground: .warn)
                    .padding(.horizontal, -16)
            }
            Spacer(minLength: 12)
            PrimaryPress(press: view.refunded) { send(.confirmRefund(orderId: view.orderId)) }
            LinkButton(label: view.notYet) { send(.dismissRefund) }
        }
        .padding(.horizontal, 20)
        .padding(.top, 24)
        .padding(.bottom, 12)
        .foregroundStyle(Palette.ink)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(Palette.ground)
    }
}
