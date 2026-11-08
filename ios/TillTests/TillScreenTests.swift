import SwiftUI
import Testing
import UserNotifications
@testable import Till

/// Phase I3's checks of the till's glue: which screen draws each `Screen`,
/// the engine's colour roles as the palette's colours, what a page's
/// buttons ask of the engine, and a new sale's alert as iOS posts it.
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

    @Test func theEnlargedCodeIsEdgeToEdgeAndTheRestAColumn() {
        #expect(TillScreen.column(.code(orderId: "o1")) == .infinity)
        #expect(TillScreen.column(.shift) == tillColumn)
        #expect(TillScreen.column(.sale(orderId: "o1")) == tillColumn)
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
