import SwiftUI

/// The one primary action on a screen: an accent capsule with onAccent
/// text, full width, 52 pt high (docs/ios-plan.md §4). Every other button
/// is the system's glass (`.buttonStyle(.glass)`), at least 44 pt.
struct PrimaryButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.headline)
            .foregroundStyle(Palette.onAccent)
            .frame(maxWidth: .infinity, minHeight: 52)
            .padding(.horizontal, 20)
            .background(Capsule().fill(Palette.accent))
            .contentShape(Capsule())
            .opacity(isEnabled ? (configuration.isPressed ? 0.8 : 1) : 0.4)
            .scaleEffect(configuration.isPressed ? 0.98 : 1)
            .animation(.easeOut(duration: 0.15), value: configuration.isPressed)
    }
}

extension ButtonStyle where Self == PrimaryButtonStyle {
    /// The accent primary button.
    static var primary: PrimaryButtonStyle { PrimaryButtonStyle() }
}

/// The smallest target the design allows, Apple's 44 pt.
let minimumTarget: CGFloat = 44

extension View {
    /// A glass button at the design's size: the system's Liquid Glass style,
    /// large, never under 44 pt high.
    func glassButton() -> some View {
        buttonStyle(.glass)
            .controlSize(.large)
            .frame(minHeight: minimumTarget)
    }
}
