import SwiftUI

/// The one primary action on a screen: an accent capsule with onAccent
/// text, full width, 52 pt high (docs/ios-plan.md §4). Orange fills this
/// button and nothing else.
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

/// A secondary action in a page's content (Share, Done, Show the code
/// again): ink words in an outlined capsule, as the boards draw it. Liquid
/// Glass stays in the system's toolbars, out of the content.
struct SecondaryButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.headline)
            .foregroundStyle(Palette.ink)
            .frame(maxWidth: .infinity, minHeight: 50)
            .padding(.horizontal, 20)
            .background(Capsule().fill(configuration.isPressed ? Palette.chip : Color.clear))
            .overlay(Capsule().strokeBorder(Palette.outline, lineWidth: 1.5))
            .contentShape(Capsule())
            .opacity(isEnabled ? 1 : 0.4)
    }
}

extension ButtonStyle where Self == PrimaryButtonStyle {
    /// The accent primary button.
    static var primary: PrimaryButtonStyle { PrimaryButtonStyle() }
}

extension ButtonStyle where Self == SecondaryButtonStyle {
    /// The outlined secondary button.
    static var secondary: SecondaryButtonStyle { SecondaryButtonStyle() }
}

/// The smallest target the design allows, Apple's 44 pt.
let minimumTarget: CGFloat = 44
