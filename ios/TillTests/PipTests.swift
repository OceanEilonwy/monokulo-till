import CoreGraphics
import Foundation
import Testing
@testable import Till

/// Phase 3 of the iPad redesign: what the floating window shows and its
/// shape (decisions 2, 5 and 8; boards P1–P4, K1), Paid's Back to Square
/// (decision 1, A3), and the name leading the Live Activity (decision 10).
struct PipTests {
    typealias State = TillActivityAttributes.ContentState

    private let us = Locale(identifier: "en_US")
    private let expires = Date(timeIntervalSince1970: 1_761_312_000)

    private func state(_ phase: State.Phase, name: String? = "Sam", uri: String? = "monero:8Bx") -> State {
        State(
            phase: phase,
            orderId: "o1",
            sale: "#bP9m",
            amount: "24.50",
            currency: "USD",
            xmr: "0.1342",
            expiresAt: expires,
            waitingCount: 1,
            paymentUri: uri,
            name: name
        )
    }

    private let offer = PipCashOffer(orderId: "cash:p9", title: "Ana · $9.00", line: "Cash · paying in Monero?")

    @Test func watchingIsTheStripWithTheLocation() {
        let idle = State(phase: .idle, waitingCount: 0)
        let picture = PipPicture.of(idle, cash: nil, place: "Front Counter", locale: us)
        #expect(picture == .watching(place: "Front Counter"))
        #expect(picture.shape == .strip)
        #expect(picture.orderId == nil)
        #expect(!picture.paused, "nothing to play while watching")
    }

    @Test func aWaitingSaleIsTheSquareWithItsCodeAndCountdown() {
        let picture = PipPicture.of(state(.waiting), cash: offer, place: "Front Counter", locale: us)
        guard case .sale(let sale) = picture else {
            Issue.record("not a sale: \(picture)")
            return
        }
        #expect(picture.shape == .square)
        #expect(picture.orderId == "o1", "⤢ opens this sale")
        #expect(sale.name == "Sam")
        #expect(sale.amount == "$24.50")
        #expect(sale.code == "monero:8Bx")
        #expect(sale.timeLeft(now: expires.addingTimeInterval(-872)) == "14:32 left")
        #expect(sale.timeLeft(now: expires.addingTimeInterval(5)) == "0:00 left")
        #expect(!picture.paused)
    }

    @Test func aSeenSaleShowsHowFarItIsInsteadOfTheCode() {
        guard case .sale(let sale) = PipPicture.of(state(.seen), cash: nil, place: "", locale: us) else {
            Issue.record("not a sale")
            return
        }
        #expect(sale.code == nil)
        #expect(sale.timeLeft(now: expires) == nil)
        #expect(sale.progress == "Seen on the network")
    }

    @Test func paidIsTheStripWithTheAmountAndName() {
        let picture = PipPicture.of(state(.paid), cash: offer, place: "Front Counter", locale: us)
        #expect(picture == .paid(amount: "$24.50", name: "Sam"))
        #expect(picture.shape == .strip)
    }

    @Test func withoutANameTheSalesNumberLeads() {
        let picture = PipPicture.of(state(.paid, name: nil), cash: nil, place: "", locale: us)
        #expect(picture == .paid(amount: "$24.50", name: "Sale #bP9m"))
    }

    @Test func aCodeThatRanOutGoesBackToWatchingOrTheCashOffer() {
        #expect(PipPicture.of(state(.expired), cash: nil, place: "Bar", locale: us) == .watching(place: "Bar"))
        #expect(PipPicture.of(state(.expired), cash: offer, place: "Bar", locale: us) == .cash(offer))
    }

    @Test func aCashOfferIsTheStripAndPlayTakesIt() {
        let idle = State(phase: .idle, waitingCount: 0)
        let picture = PipPicture.of(idle, cash: offer, place: "Front Counter", locale: us)
        #expect(picture == .cash(offer))
        #expect(picture.shape == .strip)
        #expect(picture.paused, "the system's control shows play")
        #expect(picture.offer == offer)
        #expect(picture.orderId == "cash:p9", "⤢ opens it in Till")
    }

    @Test func noStateIsWatching() {
        #expect(PipPicture.of(nil, cash: offer, place: "Bar", locale: us) == .watching(place: "Bar"))
    }

    @Test func theCashOfferIsTheOneTheEnginePublishes() {
        let offered = OfferedCash(orderId: "cash:p2", title: "Ana · $9.00", line: "Cash · paying in Monero?")
        #expect(PipCashOffer.of(offered) == PipCashOffer(orderId: "cash:p2", title: "Ana · $9.00", line: "Cash · paying in Monero?"))
        #expect(PipCashOffer.of(nil) == nil)
    }

    @Test func theStripIsThreeToOneAndTheSaleASquare() {
        #expect(PipShape.strip.size(width: 300) == CGSize(width: 300, height: 100))
        #expect(PipShape.square.size(width: 300) == CGSize(width: 300, height: 300))
        #expect(PipShape.square.size(width: nil) == CGSize(width: 240, height: 240), "a medium window until iOS says")
        #expect(PipShape.strip.size(width: 40).width == PipShape.smallest)
    }

    @Test func belowAbout200PointsTheCodeIsntDrawn() {
        #expect(!PipShape.codeFits(width: 180))
        #expect(PipShape.codeFits(width: 200))
        #expect(PipShape.codeFits(width: 320))
        #expect(PipShape.width(renderWidth: 360, scale: 2) == 180)
        #expect(PipShape.width(renderWidth: 480, scale: 0) == 480)
    }

    @Test func theCountdownRedrawsOnlyASaleWithACode() {
        let now = expires.addingTimeInterval(-60)
        #expect(PipPicture.of(state(.waiting), cash: nil, place: "", locale: us).timeLeft(now: now) == "1:00 left")
        #expect(PipPicture.of(state(.seen), cash: nil, place: "", locale: us).timeLeft(now: now) == nil)
        #expect(PipPicture.watching(place: "").timeLeft(now: now) == nil)
    }

    @Test func backToSquareOnlyOnAnIPadAtASquareTillWithSquareInstalled() {
        #expect(BackToSquare.shown(pad: true, pointOfSale: .square, showSales: .alert, squareInstalled: true))
        #expect(BackToSquare.shown(pad: true, pointOfSale: .square, showSales: .floatingWindow, squareInstalled: true))
        #expect(!BackToSquare.shown(pad: true, pointOfSale: .square, showSales: .sideBySide, squareInstalled: true), "Square is beside")
        #expect(!BackToSquare.shown(pad: true, pointOfSale: .square, showSales: .alert, squareInstalled: false))
        #expect(!BackToSquare.shown(pad: true, pointOfSale: .clover, showSales: .alert, squareInstalled: true))
        #expect(!BackToSquare.shown(pad: true, pointOfSale: nil, showSales: .alert, squareInstalled: true))
        #expect(!BackToSquare.shown(pad: false, pointOfSale: .square, showSales: .alert, squareInstalled: true))
        #expect(BackToSquare.url.absoluteString == "square-commerce-v1://")
    }

    @Test func theNameLeadsTheLiveActivity() throws {
        #expect(state(.waiting).headline == "New sale · Sam")
        #expect(state(.paid).headline == "Paid · Sam")
        #expect(state(.seen).headline == "Sam")
        #expect(state(.waiting, name: nil).headline == "New sale · #bP9m")
        #expect(state(.waiting, name: " ").headline == "New sale · #bP9m")
        let data = try JSONEncoder().encode(state(.waiting))
        #expect(try JSONDecoder().decode(State.self, from: data).name == "Sam", "the widget gets the name")
        let engine = LiveActivityState(
            phase: .waiting, orderId: "o1", sale: "#bP9m", amount: "24.50", currency: "USD", xmr: "0.1342",
            expiresAt: 1_761_312_000, waitingCount: 1, paymentUri: nil, name: "Sam"
        )
        #expect(State(engine).name == "Sam")
    }
}
