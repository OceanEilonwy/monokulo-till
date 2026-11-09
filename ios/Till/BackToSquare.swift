import TillCore
import UIKit

/// Paid's "Back to Square" on iPad and iPhone (docs/ipad-redesign.md
/// decision 1, board A3; core.md decision 94): with a sale shown full
/// screen over Square (from its alert or the floating window's ⤢), the
/// button opens Square Point of Sale again by its URL scheme. The owner checked on an iPad that
/// `square-commerce-v1://` opens it, with no prompt from an app. Till
/// then goes back to the shift, so the next sale opens fresh.
///
/// `square-commerce-v1` is in `LSApplicationQueriesSchemes`
/// (Support/Info.plist), so `canOpenURL` can say whether Square is
/// installed; the button shows only then.
enum BackToSquare {
    static let url = URL(string: "square-commerce-v1://")!

    /// Whether Paid offers the button: where the engine offers Square
    /// (`SaleView.backToPos`, a till on Square and its own sale) and Square
    /// is installed, on any device, unless Till and Square are side by
    /// side, where Square is already on screen.
    static func shown(offered: PosMark?, showSales: ShowSales, squareInstalled: Bool) -> Bool {
        offered == .square && showSales != .sideBySide && squareInstalled
    }

    /// Square Point of Sale is on this device.
    @MainActor
    static var squareInstalled: Bool {
        UIApplication.shared.canOpenURL(url)
    }

    /// Whether `core`'s Paid offers the button now.
    @MainActor
    static func shown(_ core: Core) -> Bool {
        shown(
            offered: core.sale?.view.backToPos?.mark,
            showSales: core.showSales,
            squareInstalled: squareInstalled
        )
    }

    /// Opens Square, and the till goes back to the shift.
    @MainActor
    static func open(_ core: Core) {
        UIApplication.shared.open(url)
        core.send(.back)
    }
}
