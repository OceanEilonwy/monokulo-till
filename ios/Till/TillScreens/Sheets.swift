import SwiftUI
import TillCore
import UIKit

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

/// The refund sheet (decisions 130 and 138 of android.md), drawn from the
/// engine's `RefundSheetView`: Clover can't refund by API, so it says what
/// to refund in the point of sale's own app, and where; "I've refunded it"
/// asks the point of sale now (or takes the cashier's word, for a sale
/// another point of sale took), and "Not yet", like a swipe down, leaves
/// the sale under Needs action. The engine takes it down once refunded.
struct RefundSheetScreen: View {
    let view: RefundSheetView
    let send: Send

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Heading(text: view.title, size: 24)
            // Plain: the body names the order's own title, which may hold asterisks.
            Text(view.body)
                .font(.body)
                .fixedSize(horizontal: false, vertical: true)
            if let notSeen = view.notSeen {
                NoteCard(text: notSeen, ground: .warn)
                    .padding(.horizontal, -16)
            }
            Spacer(minLength: 12)
            PrimaryPress(press: view.refunded) { send(.confirmRefund(orderId: view.orderId)) }
            Button {
                send(.dismissRefund)
            } label: {
                Text(view.notYet)
                    .frame(maxWidth: .infinity)
            }
            .glassButton()
        }
        // "Doesn't show a refund yet" arrives after the tap: said aloud, as Android's live region.
        .onChange(of: view.notSeen) { _, notSeen in
            if let notSeen {
                UIAccessibility.post(notification: .announcement, argument: notSeen)
            }
        }
        .padding(.horizontal, 20)
        .padding(.top, 24)
        .padding(.bottom, 12)
        .foregroundStyle(Palette.ink)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(Palette.ground)
    }
}
