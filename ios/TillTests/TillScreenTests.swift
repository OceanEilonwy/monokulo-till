import SwiftUI
import Testing
import UserNotifications
@testable import Till

/// Phase I3's checks of the till's glue: which screen draws each `Screen`,
/// how the system's navigation shows the engine's screen and tells it of a
/// swipe back, how the shift's list and rows read, the engine's colour
/// roles as the palette's colours, what a page's buttons ask of the engine,
/// and a new sale's alert as iOS posts it.
@MainActor
struct TillScreenTests {
    @Test func eachTillScreenHasItsRoute() {
        #expect(Route.of(.till) == .shift)
        #expect(Route.of(.sale(orderId: "o1")) == .sale(orderId: "o1"))
        #expect(Route.of(.code(orderId: "o1")) == .code(orderId: "o1"))
        #expect(Route.of(.order(orderId: "o1")) == .order(orderId: "o1"))
        #expect(Route.of(.settings) == .settings)
        #expect(Route.of(.sale(orderId: "o1")) != .sale(orderId: "o2"))
    }

    @Test func onlyTheTillsScreensLand() {
        #expect(!Route.of(nil).lands)
        #expect(!Route.of(.welcome).lands)
        #expect(!Route.of(.ready).lands)
        #expect(Route.of(.till).lands)
        #expect(Route.of(.sale(orderId: "o1")).lands)
        #expect(Route.of(.settings).lands)
    }

    @Test func theStackIsTheShiftThenTheSaleThenItsOrder() {
        #expect(TillScreen.stackPath(.shift) == [])
        #expect(TillScreen.stackPath(.settings) == [])
        #expect(TillScreen.stackPath(.sale(orderId: "o1")) == [.sale(orderId: "o1")])
        // The code is a cover over its sale.
        #expect(TillScreen.stackPath(.code(orderId: "o1")) == [.sale(orderId: "o1")])
        #expect(TillScreen.stackPath(.order(orderId: "o1")) == [.sale(orderId: "o1"), .order(orderId: "o1")])
    }

    @Test func aSwipeBackIsAsManyBacksAsItPops() {
        #expect(TillScreen.backs(from: .sale(orderId: "o1"), toDepth: 0) == 1)
        #expect(TillScreen.backs(from: .order(orderId: "o1"), toDepth: 1) == 1)
        #expect(TillScreen.backs(from: .order(orderId: "o1"), toDepth: 0) == 2)
        // A paid sale's page under the code the engine still holds: two up to the shift.
        #expect(TillScreen.backs(from: .code(orderId: "o1"), toDepth: 0) == 2)
        #expect(TillScreen.backs(from: .shift, toDepth: 0) == 0)
        #expect(TillScreen.backs(from: .sale(orderId: "o1"), toDepth: 1) == 0)
        #expect(TillScreen.orderId(.code(orderId: "o2")) == "o2")
        #expect(TillScreen.orderId(.settings) == nil)
    }

    @Test func pagesScrollFullWidthWithTheirContentInAColumn() {
        #expect(ReadableMargins.margin(width: 1180, maxWidth: 560) == 310)
        #expect(ReadableMargins.margin(width: 390, maxWidth: 560) == 0)
        #expect(ReadableMargins.margin(width: 0, maxWidth: 560) == 0)
    }

    @Test func theCustomersNameLeadsARow() {
        let ring = Ring(step: 1, confirmations: nil, required: nil, fill: 0.25, spins: false, green: false)
        let named = ShiftRow(
            title: "Sam · $8.40", sub: "Waiting · 14:40 left · #m2Qe", tone: .link, quiet: false, icon: .ring,
            ring: ring, opens: .code, name: "Sam", amount: "$8.40"
        )
        #expect(SaleRowLook.name(named) == "Sam")
        // A core with no name for it: its one-line title, whole.
        let bare = ShiftRow(title: "Sale #m2Qe · $8.40", sub: "Waiting", tone: .link, quiet: false, icon: .ring, ring: ring, opens: .code)
        #expect(SaleRowLook.name(bare) == "Sale #m2Qe · $8.40")
    }

    @Test func aRowsLineIsInTheEnginesTone() {
        let ring = Ring(step: 2, confirmations: nil, required: nil, fill: 0.5, spins: true, green: false)
        let seen = ShiftRow(title: "Sam · $8.40", sub: "Seen on the network", tone: .muted, quiet: false, icon: .ring, ring: ring, opens: .code)
        #expect(SaleRowLook.subColour(seen) == Palette.secondary)
        let waiting = ShiftRow(title: "Sam · $8.40", sub: "Waiting", tone: .link, quiet: false, icon: .ring, ring: ring, opens: .code)
        #expect(SaleRowLook.subColour(waiting) == Palette.link)
        let paid = ShiftRow(title: "Sam · $8.40", sub: "Paid", tone: .paid, quiet: false, icon: .tick, ring: ring, opens: .details)
        #expect(SaleRowLook.subColour(paid) == Palette.paid)
    }

    @Test func theTitleMenuListsTheCountersThenAllCountersApart() {
        let mine = CounterItem(
            choice: .device(deviceId: "d1"), name: "Front iPad", sub: "This counter · alerts", count: "1 waiting", selected: true
        )
        let bar = CounterItem(choice: .device(deviceId: "d2"), name: "Bar iPad", sub: "No alerts here", count: nil, selected: false)
        let all = CounterItem(choice: .all, name: "All counters", sub: nil, count: "3 waiting", selected: false)
        let menu = CounterMenu(title: "Front iPad", heading: "Show sales from", items: [mine, bar, all])
        #expect(CounterTitleMenu.counters(menu).map(\.name) == ["Front iPad", "Bar iPad"])
        #expect(CounterTitleMenu.all(menu)?.name == "All counters")
        #expect(CounterTitleMenu.line(mine) == "This counter · alerts · 1 waiting")
        #expect(CounterTitleMenu.line(bar) == "No alerts here")
        #expect(CounterTitleMenu.line(all) == "3 waiting")
        #expect(CounterTitleMenu.line(CounterItem(choice: .otherTills, name: "Other tills", sub: nil, count: nil, selected: false)) == nil)
    }

    @Test func underTheTitleMenusCounterIsTheWholePlace() {
        let top = ShiftTop(place: "The Bean Room · Front Counter", state: "Watching", tone: .plain, said: "", mark: .square)
        #expect(ShiftTitle.subtitle(top, menu: true) == "Watching · The Bean Room · Front Counter")
        #expect(ShiftTitle.subtitle(top) == "Watching · The Bean Room")
    }

    @Test func cashButtonsAskTheEngine() {
        #expect(CashButtons.action(.takeInMonero, orderId: "cash:p1") == .takeInMonero(orderId: "cash:p1"))
        #expect(CashButtons.action(.keepAsCash, orderId: "cash:p1") == .keepAsCash(orderId: "cash:p1"))
        #expect(CashButtons.action(.paidInCash, orderId: "o1") == .paidInCash(orderId: "o1"))
        #expect(CashButtons.primary(.takeInMonero))
        #expect(!CashButtons.primary(.keepAsCash))
        #expect(!CashButtons.primary(.paidInCash))
    }

    @Test func theBarFollowsTheEnginesThirds() {
        #expect(EngineProgress.fraction(value: 0, total: 3) == 0)
        #expect(abs(EngineProgress.fraction(value: 1, total: 3) - 1.0 / 3) < 1e-9)
        #expect(EngineProgress.fraction(value: 3, total: 3) == 1)
        #expect(EngineProgress.fraction(value: 1, total: 0) == 0)
        #expect(EngineProgress.status(label: "Seen on the network", count: "1 of 3") == "Seen on the network · 1 of 3")
        #expect(EngineProgress.status(label: "Paid", count: "") == "Paid")
    }

    @Test func alertsOffIsTheEnginesWarning() {
        #expect(AlertsStatusRow.off(Fact(label: "New sales and payments", value: "Off", tone: .warn, mono: false)))
        #expect(!AlertsStatusRow.off(Fact(label: "New sales and payments", value: "On", tone: .plain, mono: false)))
    }

    @Test func theListsHeadersStartItsSections() {
        let ring = Ring(step: 1, confirmations: nil, required: nil, fill: 0.25, spins: false, green: false)
        let row = ShiftRow(title: "Sale #a · $1.00", sub: "Waiting", tone: .link, quiet: false, icon: .ring, ring: ring, opens: .code)
        let entries: [ShiftEntry] = [
            .trayHeader(label: "Waiting for payment", said: "2 waiting", count: 2),
            .row(orderId: "a", row: row, ends: .first, tray: true, closes: true),
            .row(orderId: "b", row: row, ends: .last, tray: true, closes: true),
            .fold(label: "+1", said: "Show 1 more", open: false, hidden: 1),
            .earlier(label: "Earlier"),
            .row(orderId: "c", row: row, ends: .only, tray: false, closes: false),
        ]
        let sections = ShiftSections.of(entries)
        #expect(sections.count == 2)
        #expect(sections[0].header == ShiftSection.Header(label: "Waiting for payment", said: "2 waiting", count: 2))
        #expect(sections[0].entries.count == 3)
        #expect(sections[1].header?.label == "Earlier")
        #expect(sections[1].entries.count == 1)
        // A header with nothing under it isn't drawn.
        #expect(ShiftSections.of([.earlier(label: "Earlier")]).isEmpty)
    }

    @Test func theShiftsTitleIsItsLocationAndItsLineWhetherItWatches() {
        let top = ShiftTop(place: "The Bean Room · Front Counter", state: "Watching", tone: .paid, said: "", mark: .square)
        #expect(ShiftTitle.title(top) == "Front Counter")
        #expect(ShiftTitle.subtitle(top) == "Watching · The Bean Room")
        let bare = ShiftTop(place: nil, state: "Not watching", tone: .muted, said: "", mark: nil)
        #expect(ShiftTitle.title(bare) == "Till")
        #expect(ShiftTitle.subtitle(bare) == "Not watching")
    }

    @Test func theEmptyTillsChipsSayWhatTheEnginePublishes() {
        let top = ShiftTop(place: "Front Counter", state: "Watching", tone: .paid, said: "", mark: .square)
        let chips = StatusChip.chips(top: top, alerts: true, bell: true)
        #expect(chips.map(\.text) == ["Watching Square", "Alerts on", "Instant with the Bell"])
        #expect(chips[0].glyph == .watching)
        let off = StatusChip.chips(top: ShiftTop(place: nil, state: "Not watching", tone: .muted, said: "", mark: .square), alerts: false, bell: false)
        #expect(off.map(\.text) == ["Not watching", "Alerts off"])
        #expect(off[1].warn)
        #expect(StatusChip.chips(top: nil, alerts: nil, bell: nil).isEmpty)
    }

    @Test func aPaymentsBarFollowsTheTillLogo() {
        #expect(PaymentSteps.fraction(step: 1) == 0)
        #expect(abs(PaymentSteps.fraction(step: 2) - 1.0 / 3) < 1e-9)
        #expect(abs(PaymentSteps.fraction(step: 3) - 2.0 / 3) < 1e-9)
        #expect(PaymentSteps.fraction(step: 4) == 1)
    }

    @Test func shareSendsTheMoneroPaymentLink() {
        let uri = "monero:888tN?tx_amount=0.05552"
        #expect(SaleShare.uri(code: uri, address: "888tN") == uri)
        #expect(SaleShare.uri(code: "ORF1:…", address: "888tN") == "monero:888tN")
        #expect(SaleShare.preview(amount: "$13.88") == "Monero payment · $13.88")
    }

    @Test func readysChecksDifferByShapeNotOnlyColour() {
        #expect(ReadyPage.symbol(done: true) != ReadyPage.symbol(done: false))
        #expect(ReadyPage.symbol(done: false) == "circle.dashed")
    }

    @Test func aFilterSaysItsCount() {
        #expect(FilterPicker.label(FilterChip(filter: .needsAction, label: "Needs action", count: 2, selected: false)) == "Needs action 2")
        #expect(FilterPicker.label(FilterChip(filter: .all, label: "All", count: nil, selected: true)) == "All")
    }

    @Test func tonesAreThePalettesColours() {
        #expect(Roles.ink(.plain) == Palette.ink)
        #expect(Roles.ink(.muted) == Palette.secondary)
        #expect(Roles.ink(.paid) == Palette.paid)
        #expect(Roles.ink(.warn) == Palette.expiredInk)
        #expect(Roles.ink(.link) == Palette.link)
        #expect(Roles.ink(.danger) == Palette.danger)
    }

    @Test func groundsAreThePalettesColours() {
        #expect(Roles.ground(.paid) == Palette.paidGround)
        #expect(Roles.ground(.warn) == Palette.expiredGround)
        #expect(Roles.ground(.danger) == Palette.dangerGround)
        #expect(Roles.ground(.chip) == Palette.chip)
        #expect(Roles.ground(.card) == Palette.card)
        #expect(Roles.onGround(.danger) == Palette.onDangerGround)
        #expect(Roles.onGround(.card) == Palette.ink)
    }

    @Test func aRowsBadgeFollowsItsGlyph() {
        #expect(Roles.badgeGround(.clock) == .warn)
        #expect(Roles.badgeGround(.warning) == .danger)
        #expect(Roles.badgeGround(.tick) == .paid)
        #expect(Roles.badgeGround(.ring) == .chip)
        #expect(Roles.badgeGround(.refund) == .chip)
        #expect(Roles.badgeGround(.close) == .chip)
        // The live ring is drawn; every other glyph is a symbol.
        #expect(Roles.symbol(.ring) == nil)
        #expect(Roles.symbol(.tick) == "checkmark")
        #expect(Roles.glyph(.close, quiet: true) == Palette.secondary)
        #expect(Roles.glyph(.close, quiet: false) == Palette.ink)
        #expect(Roles.glyph(.tick, quiet: true) == Palette.paid)
    }

    @Test func aSaleIsPaidWhenItsProgressIsFull() {
        #expect(SalePageScreen.full(value: 3, total: 3))
        #expect(!SalePageScreen.full(value: 2, total: 3))
        #expect(!SalePageScreen.full(value: 0, total: 0))
    }

    @Test func aPagesButtonsAskTheEngine() {
        #expect(SalePageScreen.action(.refund, orderId: "o1") == .refund(orderId: "o1"))
        #expect(SalePageScreen.action(.showCodeAgain, orderId: "o1") == .reopen(orderId: "o1"))
        #expect(CloseSheetScreen.ink(.refundIt) == Palette.danger)
        #expect(CloseSheetScreen.ink(.keepUp) == Palette.ink)
    }

    @Test func aNewSalesAlertIsTimeSensitiveInTheEnginesWords() throws {
        let alert = SaleAlert(
            orderId: "o1",
            title: "New Monero sale · $24.50",
            body: "Front Counter. Swipe in Till, or tap here, to show the payment code."
        )
        let content = SaleAlerts.content(alert)
        #expect(content.title == "New Monero sale · $24.50")
        #expect(content.body == "Front Counter. Swipe in Till, or tap here, to show the payment code.")
        #expect(content.interruptionLevel == .timeSensitive)
        #expect(content.sound != nil)
        // Tapped, it opens its sale.
        #expect(SaleAlerts.orderId(content.userInfo) == "o1")
    }

    @Test func anAlertWithoutASaleOpensNothing() {
        #expect(SaleAlerts.orderId([:]) == nil)
        #expect(SaleAlerts.orderId(["orderId": 7]) == nil)
    }
}
