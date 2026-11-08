import SwiftUI

/// Phase I0's screen: proof that the Rust core runs on the device, with
/// phase I1's parts one tap away.
struct CoreView: View {
    let core: Core

    var body: some View {
        NavigationStack {
            List {
                Section("Core") {
                    LabeledContent("Version", value: core.version ?? "Starting…")
                }
                Section("Look") {
                    NavigationLink("Parts") { PartsView() }
                }
                Section("Updates") {
                    ForEach(Array(core.updates.enumerated()), id: \.offset) { _, update in
                        Text(update).font(.caption.monospaced())
                    }
                }
            }
            .navigationTitle("Till")
        }
    }
}
