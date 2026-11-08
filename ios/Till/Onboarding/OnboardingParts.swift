import SwiftUI
import TillCore

/// A step's page (the Onb* boards): an optional Back, the step's content
/// scrolling, and its buttons and step label held at the bottom.
struct StepPage<Content: View, Bottom: View>: View {
    private let back: (@MainActor () -> Void)?
    private let stepLabel: String?
    private let content: Content
    private let bottom: Bottom

    init(
        back: (@MainActor () -> Void)? = nil,
        stepLabel: String?,
        @ViewBuilder content: () -> Content,
        @ViewBuilder bottom: () -> Bottom
    ) {
        self.back = back
        self.stepLabel = stepLabel
        self.content = content()
        self.bottom = bottom()
    }

    var body: some View {
        VStack(spacing: 0) {
            if let back {
                HStack {
                    Button {
                        back()
                    } label: {
                        Image(systemName: "chevron.backward")
                            .font(.body.weight(.semibold))
                    }
                    .glassButton()
                    .accessibilityLabel("Back")
                    Spacer()
                }
                .padding(.horizontal, 16)
                .padding(.top, 8)
            }
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    content
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.bottom, 16)
            }
            .scrollBounceBehavior(.basedOnSize)
            VStack(spacing: 10) {
                bottom
                if let stepLabel {
                    Text(stepLabel)
                        .font(.footnote)
                        .foregroundStyle(Palette.secondary)
                }
            }
            .padding(.horizontal, 20)
            .padding(.top, 12)
            .padding(.bottom, 8)
        }
        .foregroundStyle(Palette.ink)
        .background(Palette.ground)
    }
}

/// A display heading: Manrope 800.
struct Heading: View {
    let text: String
    var size: CGFloat = 30
    var centred = false

    var body: some View {
        Text(text)
            .display(size)
            .multilineTextAlignment(centred ? .center : .leading)
            .fixedSize(horizontal: false, vertical: true)
            .accessibilityAddTraits(.isHeader)
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

/// "Payments go to · Shop wallet".
struct FactRow: View {
    let fact: Fact

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 12) {
            Text(fact.label)
            Spacer(minLength: 12)
            Text(fact.value)
                .font(fact.mono ? .body.monospaced() : .body)
                .foregroundStyle(toneColor(fact.tone))
                .multilineTextAlignment(.trailing)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .frame(minHeight: 48)
        .accessibilityElement(children: .combine)
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

/// The step's one primary action, as the core words it.
struct PrimaryPress: View {
    let press: Press
    let action: @MainActor () -> Void

    var body: some View {
        Button(press.label) { action() }
            .buttonStyle(.primary)
            .disabled(!press.enabled)
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

/// The paid green tick: done, connected, found.
struct DoneTick: View {
    var size: CGFloat = 24

    var body: some View {
        Image(systemName: "checkmark.circle.fill")
            .resizable()
            .scaledToFit()
            .frame(width: size, height: size)
            .foregroundStyle(Palette.paid)
            .accessibilityHidden(true)
    }
}

/// The server as a chip, with a green lock when the connection is encrypted.
struct HostChip: View {
    let host: String
    let encrypted: Bool

    var body: some View {
        HStack(spacing: 6) {
            if encrypted {
                Image(systemName: "lock.fill")
                    .font(.caption)
                    .foregroundStyle(Palette.paid)
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

/// Step 3's top: the logo, the question or the store, and the server.
struct SheetTop: View {
    let title: String
    let host: String
    let encrypted: Bool

    var body: some View {
        VStack(spacing: 10) {
            Logo(size: 64)
            Heading(text: title, size: 26, centred: true)
                .padding(.top, 6)
            HostChip(host: host, encrypted: encrypted)
        }
        .frame(maxWidth: .infinity)
        .padding(.horizontal, 24)
        .padding(.top, 32)
    }
}
