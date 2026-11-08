import SwiftUI
import TillCore

/// The widest the till's column grows: the boards are drawn at the Slide
/// Over panel's width (about 320 to 375 pt); full screen, the column stays
/// readable in the middle.
let tillColumn: CGFloat = 560

/// A screen's bar: Back, the title and its line in the middle, and what
/// the screen puts at its end.
struct TillBar<Trailing: View>: View {
    let title: String
    var line: String? = nil
    let back: @MainActor () -> Void
    private let trailing: Trailing

    init(
        title: String,
        line: String? = nil,
        back: @escaping @MainActor () -> Void,
        @ViewBuilder trailing: () -> Trailing
    ) {
        self.title = title
        self.line = line
        self.back = back
        self.trailing = trailing()
    }

    var body: some View {
        HStack(spacing: 8) {
            BackButton(action: back)
            VStack(spacing: 2) {
                Text(title)
                    .font(.headline)
                    .lineLimit(1)
                if let line, !line.isEmpty {
                    Text(line)
                        .font(.caption)
                        .foregroundStyle(Palette.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
            }
            .frame(maxWidth: .infinity)
            .accessibilityElement(children: .combine)
            .accessibilityAddTraits(.isHeader)
            trailing
                .frame(minWidth: minimumTarget)
        }
        .padding(.horizontal, 12)
        .padding(.top, 8)
        .padding(.bottom, 4)
        .foregroundStyle(Palette.ink)
    }
}

extension TillBar where Trailing == BarSpace {
    init(title: String, line: String? = nil, back: @escaping @MainActor () -> Void) {
        self.init(title: title, line: line, back: back) { BarSpace() }
    }
}

/// The room a bar keeps at its end when it has nothing there, so its title
/// stays in the middle.
struct BarSpace: View {
    var body: some View {
        Color.clear.frame(width: minimumTarget, height: minimumTarget)
    }
}

/// The chevron that sends Back to the engine.
struct BackButton: View {
    let action: @MainActor () -> Void

    var body: some View {
        Button {
            action()
        } label: {
            Image(systemName: "chevron.backward")
                .font(.body.weight(.semibold))
        }
        .glassButton()
        .accessibilityLabel("Back")
    }
}

/// A state's glyph on its ground: a shift row's 36 pt circle, a page's
/// 72 pt hero (Android's `StateBadge`).
struct StateBadge: View {
    let icon: StateIcon
    let ground: Ground
    var size: CGFloat = 36
    var quiet = false

    var body: some View {
        ZStack {
            Circle().fill(Roles.ground(ground))
            if let symbol = Roles.symbol(icon) {
                Image(systemName: symbol)
                    .font(.system(size: size * 0.42, weight: .semibold))
                    .foregroundStyle(Roles.glyph(icon, quiet: quiet))
            } else {
                RingGlyph(size: size * 0.6)
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}

/// A live sale's ring: the track, and the accent's first quarter.
struct RingGlyph: View {
    var size: CGFloat
    var step: Int = 1

    var body: some View {
        ZStack {
            Circle()
                .stroke(Palette.border, lineWidth: size * 0.14)
            Circle()
                .trim(from: 0, to: CGFloat(max(1, min(step, 4))) / 4)
                .stroke(Palette.accent, style: StrokeStyle(lineWidth: size * 0.14, lineCap: .round))
                .rotationEffect(.degrees(-90))
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}

/// What was bought (the design's order card): the receipt's summary, its
/// lines, "+N more" (which opens every line), and the foot and total.
struct OrderCardSection: View {
    let card: OrderCard
    var more: (@MainActor () -> Void)? = nil

    var body: some View {
        Card {
            VStack(alignment: .leading, spacing: 8) {
                Text(card.summary)
                    .font(.footnote)
                    .foregroundStyle(Palette.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                ForEach(Array(card.lines.enumerated()), id: \.offset) { _, line in
                    HStack(alignment: .firstTextBaseline, spacing: 12) {
                        Text(line.label)
                            .fixedSize(horizontal: false, vertical: true)
                        Spacer(minLength: 8)
                        Text(line.price)
                            .monospacedDigit()
                    }
                    .font(.subheadline)
                    .accessibilityElement(children: .combine)
                }
                if let label = card.more {
                    if let more {
                        Button(label) { more() }
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(Palette.link)
                            .buttonStyle(.plain)
                            .frame(minHeight: minimumTarget)
                    } else {
                        Text(label)
                            .font(.subheadline)
                            .foregroundStyle(Palette.secondary)
                    }
                }
                Rectangle()
                    .fill(Palette.hairline)
                    .frame(height: 1)
                HStack(alignment: .firstTextBaseline) {
                    Text(card.foot)
                        .foregroundStyle(Palette.secondary)
                    Spacer(minLength: 8)
                    Text(card.total)
                        .fontWeight(.semibold)
                        .monospacedDigit()
                }
                .font(.subheadline)
                .accessibilityElement(children: .combine)
            }
            .padding(16)
        }
    }
}

/// A note on its ground: a page's few sentences.
struct NoteCard: View {
    let text: String
    let ground: Ground

    var body: some View {
        Text(text)
            .font(.subheadline)
            .foregroundStyle(Roles.onGround(ground))
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(14)
            .background(RoundedRectangle(cornerRadius: 12).fill(Roles.ground(ground)))
            .padding(.horizontal, 16)
    }
}

/// A screen waiting for the engine's first record of it.
struct TillLoading: View {
    var body: some View {
        LoadingMark(size: 64, label: "Loading")
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
