import SwiftUI
import TillCore

/// A step's page (the Onb* boards): an optional Back in the system's
/// toolbar, the step's content scrolling across the whole width (held to a
/// readable column), and only its buttons held at the bottom, in the safe
/// area's inset, so the scrolling text never hides under them. No "Step N
/// of M" anywhere (docs/ipad-redesign.md, decision 11): iPad's rail and
/// iPhone's thin bar show the step.
struct StepPage<Content: View, Bottom: View>: View {
    private let back: (@MainActor () -> Void)?
    private let content: Content
    private let bottom: Bottom

    init(
        back: (@MainActor () -> Void)? = nil,
        @ViewBuilder content: () -> Content,
        @ViewBuilder bottom: () -> Bottom
    ) {
        self.back = back
        self.content = content()
        self.bottom = bottom()
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                content
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.bottom, 16)
        }
        .readableWidth(onboardingColumn)
        .safeAreaInset(edge: .bottom) {
            VStack(spacing: 10) {
                bottom
            }
            .frame(maxWidth: onboardingColumn)
            .padding(.horizontal, 20)
            .padding(.top, 12)
            .padding(.bottom, 8)
            .frame(maxWidth: .infinity)
            .background(Palette.ground)
        }
        .toolbar {
            if let back {
                ToolbarItem(placement: .topBarLeading) {
                    Button {
                        back()
                    } label: {
                        Label("Back", systemImage: "chevron.backward")
                    }
                }
            }
        }
        .foregroundStyle(Palette.ink)
        .background(Palette.ground)
    }
}

/// The widest an onboarding page's content grows.
let onboardingColumn: CGFloat = 640

/// A heading: the system's bold title. Manrope is kept for amounts and
/// the Welcome and Ready titles, which pass `display` (decision 20).
struct Heading: View {
    let text: String
    var centred = false
    /// Manrope 800 at this size, for Welcome and Ready.
    var display: CGFloat? = nil

    var body: some View {
        styled
            .multilineTextAlignment(centred ? .center : .leading)
            .fixedSize(horizontal: false, vertical: true)
            .accessibilityAddTraits(.isHeader)
    }

    @ViewBuilder
    private var styled: some View {
        if let display {
            Text(text).display(display)
        } else {
            Text(text).font(.title2.weight(.bold))
        }
    }
}

/// The paragraph under a heading.
struct Lead: View {
    let text: String
    var centred = false

    var body: some View {
        Text(text)
            .font(.body)
            .foregroundStyle(Palette.secondary)
            .multilineTextAlignment(centred ? .center : .leading)
            .fixedSize(horizontal: false, vertical: true)
    }
}

/// A heading and lead at the top of a page.
struct PageTop: View {
    let heading: String
    var lead: String? = nil

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Heading(text: heading)
            if let lead {
                Lead(text: lead)
            }
        }
        .padding(.horizontal, 20)
        .padding(.top, 12)
    }
}

/// The small capitals over a card.
struct SectionLabel: View {
    let text: String

    var body: some View {
        Text(text.uppercased())
            .font(.footnote)
            .foregroundStyle(Palette.secondary)
            .padding(.horizontal, 32)
            .padding(.top, 24)
            .padding(.bottom, 6)
            .accessibilityAddTraits(.isHeader)
    }
}

/// Small print under a card.
struct Footnote: View {
    let text: String

    var body: some View {
        Text(text)
            .font(.footnote)
            .foregroundStyle(Palette.secondary)
            .fixedSize(horizontal: false, vertical: true)
            .padding(.horizontal, 32)
            .padding(.top, 8)
    }
}

/// What went wrong, when something did.
struct ProblemNote: View {
    let text: String?

    var body: some View {
        if let text {
            Text(text)
                .font(.subheadline)
                .foregroundStyle(Palette.danger)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(12)
                .background(RoundedRectangle(cornerRadius: 12).fill(Palette.danger.opacity(0.1)))
                .padding(.horizontal, 16)
                .padding(.top, 12)
        }
    }
}

/// A white card of rows with hairlines between them.
struct Card<Content: View>: View {
    private let content: Content

    init(@ViewBuilder content: () -> Content) {
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            content
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 12).fill(Palette.card))
        .padding(.horizontal, 16)
    }
}

/// The line between two rows of a card.
struct Hairline: View {
    var body: some View {
        Rectangle()
            .fill(Palette.hairline)
            .frame(height: 1)
            .padding(.leading, 16)
    }
}

/// A fact's colour, by its tone.
func toneColor(_ tone: Tone) -> Color {
    switch tone {
    case .plain, .muted: Palette.secondary
    case .paid: Palette.paid
    case .warn: Palette.expiredInk
    case .link: Palette.link
    case .danger: Palette.danger
    }
}

/// "Payments go to · Shop wallet": side by side, stacked when they don't fit.
struct FactRow: View {
    let fact: Fact

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(alignment: .firstTextBaseline, spacing: 12) {
                Text(fact.label)
                    .fixedSize()
                Spacer(minLength: 12)
                value
                    .multilineTextAlignment(.trailing)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(fact.label)
                value
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .frame(minHeight: 48)
        .accessibilityElement(children: .combine)
    }

    private var value: some View {
        Text(fact.value)
            .font(fact.mono ? .body.monospaced() : .body)
            .foregroundStyle(toneColor(fact.tone))
    }
}

/// Facts on a card.
struct FactCard: View {
    let facts: [Fact]

    var body: some View {
        Card {
            ForEach(Array(facts.enumerated()), id: \.offset) { index, fact in
                if index > 0 {
                    Hairline()
                }
                FactRow(fact: fact)
            }
        }
    }
}

/// The step's one primary action, as the core words it. While the work it
/// started runs (`Press.busy`), the button shows the progress in itself.
struct PrimaryPress: View {
    let press: Press
    let action: @MainActor () -> Void

    var body: some View {
        Button {
            action()
        } label: {
            HStack(spacing: 10) {
                if press.busy {
                    ProgressView()
                        .tint(Palette.onAccent)
                        .accessibilityHidden(true)
                }
                Text(press.label)
                    .lineLimit(1)
            }
        }
        .buttonStyle(.primary)
        .disabled(!press.enabled)
        .accessibilityValue(press.busy ? "In progress" : "")
    }
}

/// A quieter action under the primary one: link-coloured words.
struct LinkPress: View {
    let press: Press
    let action: @MainActor () -> Void

    var body: some View {
        LinkButton(label: press.label, enabled: press.enabled) { action() }
    }
}

/// Link-coloured words that do something, at least 44 pt high.
struct LinkButton: View {
    let label: String
    var enabled = true
    let action: @MainActor () -> Void

    var body: some View {
        Button {
            action()
        } label: {
            Text(label)
                .font(.subheadline.weight(.semibold))
                .multilineTextAlignment(.center)
                .frame(maxWidth: .infinity, minHeight: minimumTarget)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(Palette.link)
        .disabled(!enabled)
        .opacity(enabled ? 1 : 0.4)
    }
}

/// The tick for done, connected, found: in ink, as green means paid and
/// nothing else (decision 20).
struct DoneTick: View {
    var size: CGFloat = 24

    var body: some View {
        Image(systemName: "checkmark.circle.fill")
            .resizable()
            .scaledToFit()
            .frame(width: size, height: size)
            .foregroundStyle(Palette.ink)
            .accessibilityHidden(true)
    }
}

/// The server as a chip, with a lock when the connection is encrypted.
struct HostChip: View {
    let host: String
    let encrypted: Bool

    var body: some View {
        HStack(spacing: 6) {
            if encrypted {
                Image(systemName: "lock.fill")
                    .font(.caption)
                    .foregroundStyle(Palette.secondary)
            }
            Text(host)
                .font(.subheadline.weight(.semibold))
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .background(Capsule().fill(Palette.card))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(spoken)
    }

    private var spoken: String {
        encrypted ? "\(host), encrypted" : "\(host), not encrypted"
    }
}

/// Step 3's top: the question or the store, and the server.
struct SheetTop: View {
    let title: String
    let host: String
    let encrypted: Bool

    var body: some View {
        VStack(spacing: 10) {
            Heading(text: title, centred: true)
            HostChip(host: host, encrypted: encrypted)
        }
        .frame(maxWidth: .infinity)
        .padding(.horizontal, 24)
        .padding(.top, 32)
    }
}
