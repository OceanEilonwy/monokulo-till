import SwiftUI
import Testing
import UIKit
@testable import Till

/// The live sale beside the shift's sales, drawn in windows the size of
/// every iPad's, landscape and portrait, split and Stage Manager widths,
/// light and dark (docs/ipad-redesign.md, decisions 27–31). Each window is
/// drawn for real, its system bars and lists included, and must come out
/// the size it was given. When `SNAPSHOT_DIR` is set (CI sets it through
/// `TEST_RUNNER_SNAPSHOT_DIR`), each picture is written there as a PNG, so
/// the layout can be looked at without a device.
@MainActor
struct LayoutSnapshotTests {
    struct Window {
        let name: String
        let size: CGSize
        let regular: Bool
        var dark = false
    }

    /// Points, as iPadOS gives a full-screen window or a Stage Manager or
    /// split one.
    static let windows: [Window] = [
        // 9.7-inch iPad with a home button: the owner's test device.
        Window(name: "ipad-9.7-landscape", size: CGSize(width: 1024, height: 768), regular: true),
        Window(name: "ipad-9.7-landscape-dark", size: CGSize(width: 1024, height: 768), regular: true, dark: true),
        Window(name: "ipad-9.7-portrait", size: CGSize(width: 768, height: 1024), regular: true),
        // iPad mini, the smallest iPad.
        Window(name: "ipad-mini-landscape", size: CGSize(width: 1133, height: 744), regular: true),
        Window(name: "ipad-mini-portrait", size: CGSize(width: 744, height: 1133), regular: true),
        // 13-inch iPad Pro, the largest.
        Window(name: "ipad-13-landscape", size: CGSize(width: 1376, height: 1032), regular: true),
        // Half of a 13-inch iPad in split view: regular, but the sale's
        // column is narrow beside the sidebar.
        Window(name: "ipad-13-half", size: CGSize(width: 683, height: 1032), regular: true),
        // A Stage Manager window, short and fairly narrow.
        Window(name: "stage-manager-medium", size: CGSize(width: 800, height: 600), regular: true),
        // A third of the screen, or a small Stage Manager window: compact.
        Window(name: "ipad-compact-third", size: CGSize(width: 375, height: 768), regular: false),
    ]

    @Test func theLiveSaleFitsEveryIPadWindow() async throws {
        guard UIDevice.current.userInterfaceIdiom == .pad,
              let scene = UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene }).first
        else { return }
        let folder = ProcessInfo.processInfo.environment["SNAPSHOT_DIR"].map { URL(fileURLWithPath: $0) }
        if let folder {
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        }
        for spec in Self.windows {
            let window = UIWindow(windowScene: scene)
            window.frame = CGRect(origin: .zero, size: spec.size)
            window.traitOverrides.horizontalSizeClass = spec.regular ? .regular : .compact
            window.overrideUserInterfaceStyle = spec.dark ? .dark : .light
            window.rootViewController = UIHostingController(rootView: Self.screen(regular: spec.regular))
            window.isHidden = false
            window.layoutIfNeeded()
            // Let the split view, the toolbar and the list settle.
            try await Task.sleep(for: .milliseconds(800))
            let image = UIGraphicsImageRenderer(bounds: window.bounds).image { _ in
                _ = window.drawHierarchy(in: window.bounds, afterScreenUpdates: true)
            }
            window.isHidden = true
            window.rootViewController = nil
            #expect(image.size == spec.size, "\(spec.name)")
            if let folder, let png = image.pngData() {
                try png.write(to: folder.appendingPathComponent("\(spec.name).png"))
            }
        }
    }

    /// The till as `TillScreen` lays it out: the split view on regular
    /// width, the stack on compact.
    @ViewBuilder
    static func screen(regular: Bool) -> some View {
        let send: Send = { _ in }
        if regular {
            NavigationSplitView(columnVisibility: .constant(.all)) {
                ShiftScreen(view: shift, sidebar: true, selected: "m2Qe", send: send)
                    .navigationSplitViewColumnWidth(min: 300, ideal: 340, max: 420)
            } detail: {
                NavigationStack {
                    SaleScreen(orderId: "m2Qe", view: sale, send: send)
                }
            }
            .navigationSplitViewStyle(.balanced)
            .background(Palette.ground)
        } else {
            NavigationStack {
                SaleScreen(orderId: "m2Qe", view: sale, send: send)
            }
            .background(Palette.ground)
        }
    }

    // MARK: The owner's screenshot, as the engine would write it

    static let waiting = Ring(step: 1, confirmations: nil, required: nil, fill: 0.25, spins: false, green: false)
    static let paid = Ring(step: 4, confirmations: nil, required: nil, fill: 1, spins: false, green: true)

    static let sale = SaleView(
        title: "Sam",
        titleLine: "Sale #m2Qe · 14:40 left · Front iPad",
        amount: "A$12.75",
        xmr: "0.05552 XMR",
        code: "monero:4AdUndXHHZ6cfufTMvppY6JwXNouMBzSkbLYfpAV5Usx3skxNgYeYTRj5UzqtReoS44qo9mtmXCqY45DJ852K5Jv2684Rge?tx_amount=0.05552&tx_description=m2Qe",
        codeSaid: "Payment code for A$12.75",
        spent: false,
        status: "Waiting for payment",
        hint: "Scan with any Monero wallet · 14:40 left",
        step: 1,
        ring: waiting,
        order: OrderCard(
            summary: "Square receipt · 3 items",
            lines: [
                OrderLine(label: "2 × Latte", price: "9.00"),
                OrderLine(label: "1 × Blueberry scone, warmed", price: "3.75"),
            ],
            more: "+1 more",
            foot: "Total",
            total: "12.75 AUD"
        ),
        copyAddress: Press(label: "Copy address", enabled: true),
        address: "4AdUndXHHZ6cfufTMvppY6JwXNouMBzSkbLYfpAV5Usx3skxNgYeYTRj5UzqtReoS44qo9mtmXCqY45DJ852K5Jv2684Rge",
        backToPos: nil,
        closes: true,
        ticks: true,
        progress: .init(
            value: 0, total: 3, label: "Waiting for payment", count: "0 of 3",
            said: "Waiting for payment, 0 of 3", line: "Waiting for payment · 0 of 3"
        ),
        closeLabel: "Close the sale",
        customerView: "Customer view",
        share: "Share",
        shareTitle: "Pay A$12.75 in Monero",
        orderTitle: "Order"
    )

    static func row(_ name: String, _ sub: String, _ amount: String, tone: Tone, icon: StateIcon, ring: Ring) -> ShiftRow {
        ShiftRow(
            title: "\(name) · \(amount)", sub: sub, tone: tone, quiet: false, icon: icon,
            ring: ring, opens: icon == .tick ? .details : .code, name: name, amount: amount
        )
    }

    static let shift = ShiftView(
        top: ShiftTop(place: "The Bean Room · Front Counter", state: "Watching", tone: .plain, said: "", mark: .square),
        disconnected: nil,
        problem: nil,
        signInAgain: nil,
        summary: "2 paid · A$21.00 · since 12:00 PM",
        idle: nil,
        chips: [
            FilterChip(filter: .all, label: "All", count: nil, selected: true),
            FilterChip(filter: .needsAction, label: "Needs action", count: 1, selected: false),
            FilterChip(filter: .paid, label: "Paid", count: nil, selected: false),
        ],
        entries: [
            .trayHeader(label: "Waiting for payment", said: "1 waiting", count: 1),
            .row(
                orderId: "m2Qe",
                row: row("Sam", "Waiting · 14:40 left · #m2Qe", "A$12.75", tone: .link, icon: .ring, ring: waiting),
                ends: .only, tray: true, closes: true
            ),
            .earlier(label: "Earlier"),
            .row(
                orderId: "j1Qx",
                row: row("Priya", "Paid · 4 min ago · #j1Qx", "A$8.40", tone: .paid, icon: .tick, ring: paid),
                ends: .first, tray: false, closes: false
            ),
            .row(
                orderId: "h7Rt",
                row: row("Sale #h7Rt", "Paid · 18 min ago · no name given", "A$12.60", tone: .paid, icon: .tick, ring: paid),
                ends: .last, tray: false, closes: false
            ),
        ],
        empty: nil,
        ticks: true,
        summaryLabel: "This shift"
    )
}
