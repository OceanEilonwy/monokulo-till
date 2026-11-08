import Foundation
import Testing
@testable import Till

/// Phase I4's checks of the Live Activity's glue: the engine's state as
/// the widget's content state, the relay's JSON for it, the widget's words,
/// and the link a tap opens.
struct LiveActivityTests {
    typealias State = TillActivityAttributes.ContentState

    private func engineState(
        _ phase: ActivityPhase,
        paymentUri: String? = nil,
        waitingCount: UInt32 = 1
    ) -> LiveActivityState {
        LiveActivityState(
            phase: phase,
            orderId: "o1",
            sale: "#1042",
            amount: "24.50",
            currency: "USD",
            xmr: "0.1421",
            expiresAt: 1_761_312_000,
            waitingCount: waitingCount,
            paymentUri: paymentUri
        )
    }

    @Test func theEnginesStateIsTheContentStateFieldForField() {
        let state = State(engineState(.waiting, paymentUri: "monero:4abc", waitingCount: 2))
        #expect(state == State(
            phase: .waiting,
            orderId: "o1",
            sale: "#1042",
            amount: "24.50",
            currency: "USD",
            xmr: "0.1421",
            expiresAt: Date(timeIntervalSince1970: 1_761_312_000),
            waitingCount: 2,
            paymentUri: "monero:4abc"
        ))
    }

    @Test func idleCarriesNothingOfASale() {
        let idle = LiveActivityState(
            phase: .idle, orderId: nil, sale: nil, amount: nil, currency: nil, xmr: nil,
            expiresAt: nil, waitingCount: 0, paymentUri: nil
        )
        let state = State(idle)
        #expect(state.phase == .idle)
        #expect(state.expiresAt == nil)
        #expect(state.money() == nil)
        #expect(state.staleDate == nil)
        #expect(state.headline == nil)
        #expect(state.compact == nil)
    }

    @Test func eachPhaseMapsToItsOwn() {
        #expect(State.Phase(ActivityPhase.idle) == .idle)
        #expect(State.Phase(ActivityPhase.waiting) == .waiting)
        #expect(State.Phase(ActivityPhase.seen) == .seen)
        #expect(State.Phase(ActivityPhase.confirming) == .confirming)
        #expect(State.Phase(ActivityPhase.paid) == .paid)
        #expect(State.Phase(ActivityPhase.expired) == .expired)
    }

    @Test func theRelaysJsonDecodesWithUnixSecondsAndPaymentURI() throws {
        let json = ##"{"phase":"seen","sale":"#1042","amount":"24.50","currency":"USD","xmr":"0.1421","expiresAt":1761312000,"waitingCount":2,"paymentURI":"monero:4abc"}"##
        let state = try JSONDecoder().decode(State.self, from: Data(json.utf8))
        #expect(state.phase == .seen)
        #expect(state.orderId == nil, "Monokulo's pushes don't name the order")
        #expect(state.expiresAt == Date(timeIntervalSince1970: 1_761_312_000))
        #expect(state.waitingCount == 2)
        #expect(state.paymentUri == "monero:4abc")
    }

    @Test func aStateGoesRoundTheCoderUnchanged() throws {
        let state = State(engineState(.confirming))
        let data = try JSONEncoder().encode(state)
        #expect(try JSONDecoder().decode(State.self, from: data) == state)
        let object = try #require(try JSONSerialization.jsonObject(with: data) as? [String: Any])
        #expect(object["expiresAt"] as? Double == 1_761_312_000)
    }

    @Test func theAmountIsWrittenWithItsCurrency() {
        let state = State(engineState(.waiting))
        #expect(state.money(locale: Locale(identifier: "en_US")) == "$24.50")
        var yen = state
        yen.amount = "1200"
        yen.currency = "JPY"
        #expect(yen.money(locale: Locale(identifier: "en_US"))?.contains("1,200") == true)
    }

    @Test func onlyALiveCodeGoesStale() {
        let expires = Date(timeIntervalSince1970: 1_761_312_000)
        #expect(State(engineState(.waiting)).staleDate == expires)
        #expect(State(engineState(.seen)).staleDate == expires)
        #expect(State(engineState(.confirming)).staleDate == expires)
        #expect(State(engineState(.paid)).staleDate == nil)
        #expect(State(engineState(.expired)).staleDate == nil)
    }

    @Test func theWidgetsWordsFollowDesignSixsTable() {
        #expect(State(engineState(.waiting)).headline == "New sale · #1042")
        #expect(State(engineState(.waiting, waitingCount: 2)).headline == "New sale · #1042 · +1 more")
        #expect(State(engineState(.waiting)).status(location: "") == "Waiting for payment")
        #expect(State(engineState(.waiting)).hint == "Tap to show code")
        #expect(State(engineState(.waiting, paymentUri: "monero:4abc")).hint == "Tap to enlarge")
        #expect(State(engineState(.seen)).compact == "Seen")
        #expect(State(engineState(.seen)).status(location: "") == "Seen on the network")
        #expect(State(engineState(.confirming)).compact == "1/2")
        #expect(State(engineState(.paid)).headline == "Paid · sale #1042")
        #expect(State(engineState(.paid)).status(location: "") == "0.1421 XMR received")
        #expect(State(engineState(.expired)).status(location: "") == "No payment yet · refund in Square")
        #expect(State(engineState(.paid)).hint == nil)
        let idle = State(phase: .idle, waitingCount: 0)
        #expect(idle.status(location: "Front Counter") == "Watching for sales · Front Counter")
        #expect(idle.status(location: "") == "Watching for sales")
    }

    @Test func theLocationIsTheShiftLinesLastPart() {
        #expect(TillActivityAttributes.location(place: "The Bean Room · Front Counter") == "Front Counter")
        #expect(TillActivityAttributes.location(place: "Front Counter") == "Front Counter")
        #expect(TillActivityAttributes.location(place: nil) == "")
        let attributes = TillActivityAttributes(store: "The Bean Room", place: "The Bean Room · Front Counter")
        #expect(attributes.storeName == "The Bean Room")
        #expect(attributes.location == "Front Counter")
    }

    @Test func aTapOpensItsSale() throws {
        let url = TillActivityLink.url(orderId: "o1")
        #expect(url.absoluteString == "monokulo-till://sale/o1")
        #expect(TillActivityLink.isSale(url))
        #expect(TillActivityLink.orderId(url) == "o1")
        // After a push the state names no order: the tap only brings Till forward.
        let bare = TillActivityLink.url(orderId: nil)
        #expect(bare.absoluteString == "monokulo-till://sale")
        #expect(TillActivityLink.isSale(bare))
        #expect(TillActivityLink.orderId(bare) == nil)
        // An id with a space survives the trip.
        #expect(TillActivityLink.orderId(TillActivityLink.url(orderId: "a b")) == "a b")
        // Other links aren't the activity's.
        #expect(!TillActivityLink.isSale(try #require(URL(string: "monokulo-till://square?code=x"))))
        #expect(!TillActivityLink.isSale(try #require(URL(string: "https://till.monokulo.com/connect#x"))))
    }
}
