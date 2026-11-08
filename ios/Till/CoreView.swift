import SwiftUI

/// Phase I0's only screen: proof that the Rust core runs on the device.
struct CoreView: View {
    let core: Core

    var body: some View {
        NavigationStack {
            List {
                Section("Core") {
                    LabeledContent("Version", value: core.version ?? "Starting…")
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
