import SwiftUI
import TillCore

/// The till (phase I3), once setup is done: the screen the engine's
/// `Screen` names, drawn from its records, with the close and refund
/// sheets over it while the engine has them up. Laid out for the Slide
/// Over panel's width; full screen, the column stays in the middle.
struct TillScreen: View {
    let core: Core
    let route: Route

    /// The sheets, up while the engine has them up.
    @State private var closeUp = false
    @State private var refundUp = false

    var body: some View {
        content
            // The enlarged code is white edge to edge; the rest keep a column.
            .frame(maxWidth: TillScreen.column(route))
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Palette.ground)
            // Dismissed by a swipe, a sheet went without an answer.
            .sheet(isPresented: $closeUp, onDismiss: { if core.closeSheet != nil { core.send(.dismissClose) } }) {
                if let sheet = core.closeSheet {
                    CloseSheetScreen(sheet: sheet, send: send)
                        .presentationDetents([.medium, .large])
                        .presentationBackground(Palette.ground)
                }
            }
            .sheet(isPresented: $refundUp, onDismiss: { if core.refundSheet != nil { core.send(.dismissRefund) } }) {
                if let view = core.refundSheet {
                    RefundSheetScreen(view: view, send: send)
                        .presentationDetents([.medium, .large])
                        .presentationBackground(Palette.ground)
                }
            }
            .onChange(of: core.closeSheet != nil, initial: true) { _, up in closeUp = up }
            .onChange(of: core.refundSheet != nil, initial: true) { _, up in refundUp = up }
    }

    /// How wide `route`'s screen grows.
    static func column(_ route: Route) -> CGFloat {
        if case .code = route { return .infinity }
        return tillColumn
    }

    /// The cashier's actions, to the engine.
    private var send: Send {
        { action in core.send(action) }
    }

    @ViewBuilder
    private var content: some View {
        switch route {
        case .shift:
            if let shift = core.shift {
                ShiftScreen(view: shift, send: send)
            } else {
                TillLoading()
            }
        case .sale(let orderId):
            if let sale = core.sale, sale.orderId == orderId {
                if sale.code.done {
                    SalePageScreen(orderId: orderId, page: sale.page, send: send)
                } else {
                    SaleScreen(orderId: orderId, view: sale.view, send: send)
                }
            } else {
                TillLoading()
            }
        case .code(let orderId):
            if let sale = core.sale, sale.orderId == orderId {
                if sale.code.done {
                    SalePageScreen(orderId: orderId, page: sale.page, send: send)
                } else {
                    CodeScreen(view: sale.code, send: send)
                }
            } else {
                TillLoading()
            }
        case .order(let orderId):
            if let sale = core.sale, sale.orderId == orderId {
                OrderScreen(title: sale.view.title, order: sale.order, send: send)
            } else {
                TillLoading()
            }
        case .settings:
            if let settings = core.settings {
                SettingsScreen(view: settings, wakeUps: core.wakeUps, send: send)
            } else {
                TillLoading()
            }
        case .starting, .onboarding:
            TillLoading()
        }
    }
}
