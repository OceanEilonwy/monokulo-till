import SwiftUI
import TillCore

/// Prices (docs/view-key-mode.md §8): where a till on the shop's own
/// wallet gets the price of XMR, as Fob's Price from asks it. Fob's intro
/// with the currency in bold, each provider that prices it with what it
/// says XMR is worth (or why it gave nothing, in the warning colour, not
/// pickable), and what a request gives away. No canvas board draws it: the
/// rest of setup's look.
struct PricesPage: View {
    let step: PricesStep
    let view: OnboardingView
    let send: Send

    var body: some View {
        StepPage(back: { send(.back) }) {
            PageTop(heading: view.heading, lead: view.lead)
            intro
                .padding(.horizontal, 20)
                .padding(.top, 16)
            SectionLabel(text: step.section)
            Card {
                if let asking = step.asking {
                    HStack(spacing: 12) {
                        ProgressView()
                        Text(asking)
                            .font(.subheadline)
                            .foregroundStyle(Palette.secondary)
                    }
                    .padding(.horizontal, 16)
                    .frame(maxWidth: .infinity, minHeight: 52, alignment: .leading)
                    .accessibilityElement(children: .combine)
                    Hairline()
                }
                ForEach(Array(step.providers.enumerated()), id: \.offset) { index, row in
                    if index > 0 {
                        Hairline()
                    }
                    ProviderLine(row: row, padded: true) {
                        send(.choosePriceProvider(name: row.name))
                    }
                }
            }
            .accessibilityValue(step.summary)
            if let fob = step.fromFob {
                Footnote(text: fob)
            }
            Footnote(text: step.footnote)
            ProblemNote(text: step.problem)
        } bottom: {
            PrimaryPress(press: step.next) { send(.next) }
        }
    }

    /// "Prices in **Australian dollars**, checked as this page opens. Tap a
    /// provider to pick it."
    private var intro: some View {
        Text(PricesPage.intro(step))
            .font(.subheadline)
            .foregroundStyle(Palette.secondary)
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// The intro, its currency in bold and in ink.
    static func intro(_ step: PricesStep) -> AttributedString {
        var bold = AttributedString(step.introBold)
        bold.font = .subheadline.weight(.semibold)
        bold.foregroundColor = Palette.ink
        return AttributedString(step.introBefore) + bold + AttributedString(step.introAfter)
    }
}

/// One provider on one line, as Fob draws it: its name, what it says XMR
/// is worth at the end (or why it gave nothing, in the warning colour), and
/// a tick on the one in use. On Prices' card (`padded`) and in Settings.
struct ProviderLine: View {
    let row: ProviderRow
    var padded = false
    let action: @MainActor () -> Void

    var body: some View {
        Button {
            action()
        } label: {
            HStack(spacing: 12) {
                Text(row.name)
                    .foregroundStyle(row.enabled || row.selected ? Palette.ink : Palette.secondary)
                Spacer(minLength: 8)
                if let value = row.value {
                    Text(value)
                        .font(.subheadline)
                        .foregroundStyle(row.failed ? Palette.danger : Palette.secondary)
                        .multilineTextAlignment(.trailing)
                }
                Image(systemName: "checkmark")
                    .font(.body.weight(.semibold))
                    .foregroundStyle(Palette.link)
                    .opacity(row.selected ? 1 : 0)
                    .accessibilityHidden(true)
            }
            .padding(.horizontal, padded ? 16 : 0)
            .padding(.vertical, padded ? 10 : 0)
            .frame(minHeight: padded ? 52 : minimumTarget)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!row.enabled)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(row.label)
        .accessibilityAddTraits(row.selected ? [.isButton, .isSelected] : .isButton)
    }
}
