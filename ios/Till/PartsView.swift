import SwiftUI

/// A preview of the look and the shared parts (the Till logo, the status
/// glyphs, type, the QR code, buttons and colours): reached from
/// ``CoreView``.
struct PartsView: View {
    /// A sample payment address, in the form the till's codes take.
    nonisolated static let sampleURI = "monero:888tNkZrPN6JsEgekjMnABU4TBzc2Dt29EPAvkRxbANsAnjyPbb3iQ1YBRk1UXcdRsiKc9dhwMVgN5S9cQUiyoogDavup3H?tx_amount=0.123456789012&tx_description=Till%20sale%2042"

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 28) {
                section("Till logo") {
                    HStack(spacing: 12) {
                        ForEach(Self.states, id: \.label) { state in
                            TillWidget(state: state, size: 56)
                        }
                    }
                    HStack(spacing: 12) {
                        ForEach(Self.states, id: \.label) { state in
                            TillRound(state: state, size: 56)
                        }
                    }
                }
                section("Status glyphs") {
                    HStack(spacing: 16) {
                        ForEach(Self.glyphs, id: \.self) { kind in
                            StatusGlyph(kind: kind, size: 28)
                        }
                    }
                }
                section("Type") {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("$24.50").display(44, tracking: -1)
                        Text("Show the customer").display(24)
                        Text("SF for everything else.").font(.body).foregroundStyle(Palette.ink)
                        Text("Secondary text").font(.subheadline).foregroundStyle(Palette.secondary)
                        Text("A link").font(.subheadline).foregroundStyle(Palette.link)
                    }
                }
                section("QR") {
                    QRCodeView(text: Self.sampleURI, label: "Payment code", size: 280)
                }
                section("Buttons") {
                    VStack(spacing: 12) {
                        Button("Show the customer") {}
                            .buttonStyle(.primary)
                        Button("Share") {}
                            .buttonStyle(.secondary)
                    }
                }
                section("Colours") {
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 64))], spacing: 8) {
                        ForEach(Self.swatches, id: \.name) { swatch in
                            VStack(spacing: 4) {
                                RoundedRectangle(cornerRadius: 10)
                                    .fill(swatch.color)
                                    .frame(height: 40)
                                    .overlay(RoundedRectangle(cornerRadius: 10).stroke(Palette.border))
                                Text(swatch.name).font(.caption2).foregroundStyle(Palette.secondary)
                            }
                        }
                    }
                }
            }
            .padding(16)
            .foregroundStyle(Palette.ink)
        }
        .background(Palette.ground)
        .navigationTitle("Parts")
    }

    private static let states: [TillLogoState] = [.idle, .waiting, .seen, .confirming, .paid]
    private static let glyphs: [StatusGlyphKind] = [.watching, .waiting, .seen, .paid, .closed, .cash]

    private static let swatches: [(name: String, color: Color)] = [
        ("ground", Palette.ground), ("ink", Palette.ink), ("secondary", Palette.secondary),
        ("accent", Palette.accent), ("onAccent", Palette.onAccent), ("link", Palette.link),
        ("paid", Palette.paid), ("progress", Palette.progress), ("track", Palette.track), ("danger", Palette.danger), ("hairline", Palette.hairline),
        ("border", Palette.border), ("expired", Palette.expiredGround), ("expired ink", Palette.expiredInk),
    ]

    private func section<Content: View>(_ title: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(title).font(.headline).foregroundStyle(Palette.secondary)
            content()
        }
    }
}
