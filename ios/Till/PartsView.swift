import SwiftUI

/// Phase I1's preview of the look and the shared parts (the design's Logo,
/// LoadingMark and QR boards), for a CI snapshot: reached from ``CoreView``.
struct PartsView: View {
    /// A sample payment address, in the form the till's codes take.
    nonisolated static let sampleURI = "monero:888tNkZrPN6JsEgekjMnABU4TBzc2Dt29EPAvkRxbANsAnjyPbb3iQ1YBRk1UXcdRsiKc9dhwMVgN5S9cQUiyoogDavup3H?tx_amount=0.123456789012&tx_description=Till%20sale%2042"

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 28) {
                section("Logo") {
                    HStack(spacing: 24) {
                        Logo(size: 64, label: "Monokulo")
                        Logo(size: 64, ring: Palette.accent)
                    }
                }
                section("Loading mark") {
                    HStack(alignment: .center, spacing: 24) {
                        LoadingMark(size: 32, label: "Watching")
                        LoadingMark(size: 64, label: "Watching")
                        LoadingMark(size: 128, label: "Watching")
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
                        Button("Copy address") {}
                            .glassButton()
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

    private static let swatches: [(name: String, color: Color)] = [
        ("ground", Palette.ground), ("ink", Palette.ink), ("secondary", Palette.secondary),
        ("accent", Palette.accent), ("onAccent", Palette.onAccent), ("link", Palette.link),
        ("paid", Palette.paid), ("danger", Palette.danger), ("hairline", Palette.hairline),
        ("border", Palette.border), ("expired", Palette.expiredGround), ("expired ink", Palette.expiredInk),
    ]

    private func section<Content: View>(_ title: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(title).font(.headline).foregroundStyle(Palette.secondary)
            content()
        }
    }
}
