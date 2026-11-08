import SwiftUI

/// Text with `**bold**` runs, as the core's steps bold the names of things
/// to tap (Android's `rich`).
enum Rich {
    struct Run: Equatable {
        var text: String
        var bold: Bool
    }

    /// `text` cut at each `**`: every other run is bold.
    static func runs(_ text: String) -> [Run] {
        text.components(separatedBy: "**").enumerated().compactMap { index, part in
            part.isEmpty ? nil : Run(text: part, bold: index % 2 == 1)
        }
    }

    /// `text` as one `Text`, its bold runs strong.
    static func text(_ text: String) -> Text {
        var string = AttributedString()
        for run in runs(text) {
            var piece = AttributedString(run.text)
            if run.bold {
                piece.inlinePresentationIntent = .stronglyEmphasized
            }
            string += piece
        }
        return Text(string)
    }
}
