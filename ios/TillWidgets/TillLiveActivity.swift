import ActivityKit
import SwiftUI
import WidgetKit

/// Till's widget extension: only the Live Activity (docs/design.md §6).
/// It links no core and fetches nothing; the app and Monokulo's pushes set
/// its content state, and iOS redraws it.
@main
struct TillWidgetsBundle: WidgetBundle {
    var body: some Widget {
        TillLiveActivity()
    }
}

/// The Live Activity: the lock screen and banner, and the Dynamic Island's
/// compact, minimal and expanded faces (boards Main, PhoneOrder,
/// PhoneOrderQR and PhonePaid). A tap opens `monokulo-till://sale/{id}`.
struct TillLiveActivity: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: TillActivityAttributes.self) { context in
            LockScreenView(state: context.state, location: context.attributes.location)
                .widgetURL(TillActivityLink.url(orderId: context.state.orderId))
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    ExpandedLeading(state: context.state)
                        .dynamicIsland(verticalPlacement: .belowIfTooWide)
                }
                DynamicIslandExpandedRegion(.trailing) {
                    ExpandedTrailing(state: context.state)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    ExpandedBottom(state: context.state, location: context.attributes.location)
                }
            } compactLeading: {
                PhaseGlyph(phase: context.state.phase, size: 22, form: .round)
            } compactTrailing: {
                CompactTrailing(state: context.state)
            } minimal: {
                PhaseGlyph(phase: context.state.phase, size: 22, form: .round)
            }
            .widgetURL(TillActivityLink.url(orderId: context.state.orderId))
            .keylineTint(WidgetPalette.link)
        }
    }
}

/// The phase's glyph: Till's terminal while watching, waiting or paid (the
/// round form in the island's circles, the square one where it has room),
/// the clock once the code ran out. [dark] is the charcoal body: the island
/// is always dark, the lock screen follows the device.
struct PhaseGlyph: View {
    enum Form {
        case round, square
    }

    let phase: TillActivityAttributes.ContentState.Phase
    var size: CGFloat
    var form: Form = .square
    var dark: Bool = true

    var body: some View {
        switch phase {
        case .expired:
            Image(systemName: "clock")
                .resizable()
                .scaledToFit()
                .frame(width: size, height: size)
                .foregroundStyle(WidgetPalette.expired)
                .accessibilityLabel("No payment yet")
        case .idle, .waiting, .seen, .confirming, .paid:
            switch form {
            case .round: WidgetRound(status: TillStatus(phase), size: size, dark: dark)
            case .square: WidgetTerminal(status: TillStatus(phase), size: size, dark: dark)
            }
        }
    }
}

/// The compact island's end: a green dot while watching, else the amount,
/// "Seen" or "1/2".
struct CompactTrailing: View {
    let state: TillActivityAttributes.ContentState

    var body: some View {
        if let words = state.compact {
            Text(words)
                .font(.system(size: 15, weight: .semibold))
                .monospacedDigit()
                .lineLimit(1)
                .foregroundStyle(state.phase == .paid ? WidgetPalette.green : WidgetPalette.cream)
        } else {
            Circle()
                .fill(WidgetPalette.green)
                .frame(width: 7, height: 7)
                .accessibilityLabel("Watching")
        }
    }
}

/// The expanded island's start: the glyph, or the code when the island
/// shows it (PhoneOrderQR).
struct ExpandedLeading: View {
    let state: TillActivityAttributes.ContentState

    var body: some View {
        if let uri = state.paymentUri, state.phase == .waiting {
            WidgetQR(text: uri, size: 96)
        } else {
            PhaseGlyph(phase: state.phase, size: 52)
                .padding(.leading, 4)
        }
    }
}

/// The expanded island's end: the amount, large, and the XMR under it.
struct ExpandedTrailing: View {
    let state: TillActivityAttributes.ContentState

    var body: some View {
        if let money = state.money() {
            VStack(alignment: .trailing, spacing: 2) {
                Text(money)
                    .font(.system(size: 28, weight: .heavy))
                    .monospacedDigit()
                    .lineLimit(1)
                    .minimumScaleFactor(0.6)
                    .foregroundStyle(WidgetPalette.cream)
                if let xmr = state.xmr {
                    Text("\(xmr) XMR")
                        .font(.footnote)
                        .monospacedDigit()
                        .foregroundStyle(WidgetPalette.muted)
                }
            }
            .padding(.trailing, 4)
        }
    }
}

/// The expanded island's words: the headline, the status, the countdown
/// while a code is up, and what a tap does.
struct ExpandedBottom: View {
    let state: TillActivityAttributes.ContentState
    let location: String

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            if let headline = state.headline {
                Text(headline)
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(WidgetPalette.muted)
                    .lineLimit(1)
            }
            Text(state.status(location: location))
                .font(.headline)
                .foregroundStyle(WidgetPalette.cream)
                .lineLimit(2)
            if let expiresAt = state.expiresAt, state.phase == .waiting {
                Countdown(until: expiresAt)
                    .font(.footnote)
                    .foregroundStyle(WidgetPalette.muted)
            }
            if let hint = state.hint {
                Text(hint)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(WidgetPalette.link)
                    .padding(.top, 3)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 8)
    }
}

/// "29:41 left", counted down by iOS itself.
struct Countdown: View {
    let until: Date

    var body: some View {
        let now = Date.now
        let end = max(now, until)
        Text("\(Text(timerInterval: now...end, countsDown: true)) left")
            .monospacedDigit()
    }
}

/// The lock screen and the banner: the glyph, the words, the amount, and
/// the code when the island shows it.
struct LockScreenView: View {
    let state: TillActivityAttributes.ContentState
    let location: String

    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        let dark = colorScheme == .dark
        let ink = dark ? WidgetPalette.darkInk : WidgetPalette.ink
        let secondary = dark ? WidgetPalette.darkSecondary : WidgetPalette.secondary
        HStack(alignment: .center, spacing: 14) {
            PhaseGlyph(phase: state.phase, size: 44, dark: dark)
            VStack(alignment: .leading, spacing: 2) {
                if let headline = state.headline {
                    Text(headline)
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(secondary)
                        .lineLimit(1)
                }
                Text(state.status(location: location))
                    .font(.headline)
                    .foregroundStyle(statusColor(dark: dark, ink: ink))
                    .lineLimit(2)
                if let expiresAt = state.expiresAt, state.phase == .waiting {
                    Countdown(until: expiresAt)
                        .font(.footnote)
                        .foregroundStyle(secondary)
                }
                if let hint = state.hint {
                    Text(hint)
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(dark ? WidgetPalette.link : WidgetPalette.linkInk)
                }
            }
            Spacer(minLength: 8)
            if let money = state.money() {
                VStack(alignment: .trailing, spacing: 2) {
                    Text(money)
                        .font(.system(size: 26, weight: .heavy))
                        .monospacedDigit()
                        .lineLimit(1)
                        .minimumScaleFactor(0.6)
                        .foregroundStyle(ink)
                    if let xmr = state.xmr {
                        Text("\(xmr) XMR")
                            .font(.footnote)
                            .monospacedDigit()
                            .foregroundStyle(secondary)
                    }
                }
            }
            if let uri = state.paymentUri, state.phase == .waiting {
                WidgetQR(text: uri, size: 84)
            }
        }
        .padding(16)
        .activityBackgroundTint(dark ? WidgetPalette.darkGround : WidgetPalette.ground)
        .activitySystemActionForegroundColor(ink)
    }

    private func statusColor(dark: Bool, ink: Color) -> Color {
        switch state.phase {
        case .paid: dark ? WidgetPalette.green : WidgetPalette.paid
        case .expired: dark ? WidgetPalette.expired : WidgetPalette.expiredInk
        default: ink
        }
    }
}
