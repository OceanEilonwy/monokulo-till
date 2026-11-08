import Foundation
import Testing
@testable import Till

/// Phase I6's checks of the glue's pure pieces for a second point of sale:
/// which one the cashier chose, as the views say; where its sign-in comes
/// back; and which of a sale page's buttons leads.
@MainActor
struct CloverTests {
    private let square = "https://till.monokulo.com/square"
    private let clover = "https://till.monokulo.com/clover"

    private func choice(_ mark: PosMark, selected: Bool) -> PosChoice {
        PosChoice(
            mark: mark,
            markInInk: mark == .square,
            choice: Choice(label: mark == .square ? "Square" : "Clover", detail: nil, selected: selected, enabled: true)
        )
    }

    private func step(choosing chosen: PosMark?) -> PosStep {
        PosStep(
            back: false,
            choices: [choice(.square, selected: chosen == .square), choice(.clover, selected: chosen == .clover)],
            card: nil,
            next: nil,
            skip: nil
        )
    }

    private func fact(_ label: String, _ value: String) -> Fact {
        Fact(label: label, value: value, tone: .plain, mono: false)
    }

    @Test func stepFoursTickedRowIsTheChoice() {
        #expect(chosenPos(step: step(choosing: .clover), settings: nil) == .clover)
        #expect(chosenPos(step: step(choosing: .square), settings: nil) == .square)
        #expect(chosenPos(step: step(choosing: nil), settings: nil) == nil)
    }

    @Test func stepFourOutweighsSettingsKeptFromBefore() {
        let before = PosSection.choosing(choices: [choice(.square, selected: true)], signIn: "Sign in to Square", footnote: "")
        #expect(chosenPos(step: step(choosing: .clover), settings: before) == .clover)
    }

    @Test func settingsSayTheChoiceSignedInOrNot() {
        let signedIn = PosSection.signedIn(
            mark: .clover, markInInk: false, name: "Clover", account: nil,
            place: fact("Merchant", "Corner Café"), changeLocation: false,
            paymentType: fact("Payment type", "Not in Clover yet"), lookAgain: "Look again",
            signOut: "Sign out of Clover", change: "Change the point of sale", footnotes: []
        )
        #expect(chosenPos(step: nil, settings: signedIn) == .clover)
        let choosing = PosSection.choosing(
            choices: [choice(.square, selected: false), choice(.clover, selected: true)],
            signIn: "Sign in to Clover", footnote: ""
        )
        #expect(chosenPos(step: nil, settings: choosing) == .clover)
        #expect(chosenPos(step: nil, settings: nil) == nil)
    }

    @Test func aSignInComesBackToTheChosenPointOfSalesRedirect() {
        #expect(redirectUri(.clover, square: square, clover: clover) == clover)
        #expect(redirectUri(.square, square: square, clover: clover) == square)
        #expect(redirectUri(nil, square: square, clover: clover) == square)
    }

    @Test func aCloverPageNamingNoRedirectEndsAtClovers() throws {
        let bare = try #require(URL(string: "https://www.clover.com/oauth/v2/authorize?client_id=x"))
        let fallback = redirectUri(.clover, square: square, clover: clover)
        #expect(SignInCallback.of(page: bare, fallback: fallback) == .https(host: "till.monokulo.com", path: "/clover"))
        let named = try #require(URL(string:
            "https://www.clover.com/oauth/v2/authorize?client_id=x&redirect_uri=https%3A%2F%2Ftill.monokulo.com%2Fclover&code_challenge=c&code_challenge_method=S256&state=s"))
        #expect(SignInCallback.of(page: named, fallback: square) == .https(host: "till.monokulo.com", path: "/clover"))
    }

    @Test func cloverReturnsWithItsMerchant() throws {
        let back = try #require(URL(string: "https://till.monokulo.com/clover?merchant_id=M1&code=abc&state=s"))
        #expect(IncomingLink.of(back, square: square, clover: clover)
            == .signInReturned("https://till.monokulo.com/clover?merchant_id=M1&code=abc&state=s"))
    }

    @Test func refundLeadsAPageAndShowCodeAgainNever() {
        #expect(SalePageScreen.primary(.refund))
        #expect(!SalePageScreen.primary(.showCodeAgain))
    }
}
