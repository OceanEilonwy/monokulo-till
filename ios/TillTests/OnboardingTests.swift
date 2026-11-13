import Foundation
import Testing
import UserNotifications
@testable import Till

/// Phase I2's checks of the glue's pure pieces: which screen draws the
/// engine's `Screen`, how a link that opened the app reaches the engine, and
/// where a sign-in page sends the browser back.
struct OnboardingTests {
    private let square = "https://till.monokulo.com/square"
    private let clover = "https://till.monokulo.com/clover"

    private func link(_ text: String) throws -> IncomingLink {
        IncomingLink.of(try #require(URL(string: text)), square: square, clover: clover, host: "till.monokulo.com")
    }

    @Test func setupScreensAreOnboardingAndTheRestTheTill() {
        #expect(Route.of(nil) == .starting)
        let setup: [Screen] = [
            .welcome, .scan, .confirm, .connected, .pointOfSale, .locations, .counter, .alerts, .float, .showSales, .ready,
            .paymentsMode, .walletEntry, .fobRequest, .fobConnected, .walletCode, .walletKeys, .walletSeed, .thisTill,
            .lightServer, .prices,
        ]
        for screen in setup {
            #expect(Route.of(screen) == .onboarding)
        }
        let till: [Screen] = [.till, .keypad, .settings, .sale(orderId: "o1"), .code(orderId: "o1"), .order(orderId: "o1")]
        for screen in till {
            #expect(Route.of(screen) != .onboarding)
            #expect(Route.of(screen) != .starting)
        }
    }

    @Test func aRedirectBackIsASignInReturnedWhole() throws {
        #expect(try link("https://till.monokulo.com/square?code=abc&state=xyz")
            == .signInReturned("https://till.monokulo.com/square?code=abc&state=xyz"))
        #expect(try link("https://till.monokulo.com/clover?code=abc&merchant_id=M1")
            == .signInReturned("https://till.monokulo.com/clover?code=abc&merchant_id=M1"))
    }

    @Test func theSchemesRedirectIsRewrittenToTheSetupsRedirect() throws {
        #expect(try link("monokulo-till://square?code=abc&state=xyz")
            == .signInReturned("https://till.monokulo.com/square?code=abc&state=xyz"))
        #expect(try link("monokulo-till-debug://clover?code=abc")
            == .signInReturned("https://till.monokulo.com/clover?code=abc"))
    }

    @Test func openInTillCarriesTheConnectLinkInItsFragment() throws {
        let wrapped = "https://till.monokulo.com/connect#https%3A%2F%2Fshop.example%2Fapps%2Fconnect%23token%3Dabc123"
        #expect(try link(wrapped) == .connect("https://shop.example/apps/connect#token=abc123"))
        // With nothing wrapped, the engine sees the link and says it isn't one.
        #expect(try link("https://till.monokulo.com/connect") == .connect("https://till.monokulo.com/connect"))
    }

    @Test func anyOtherLinkIsOfferedAsAConnectLink() throws {
        #expect(try link("https://shop.example/apps/connect#token=abc123")
            == .connect("https://shop.example/apps/connect#token=abc123"))
        #expect(try link("https://till.monokulo.com/squares") == .connect("https://till.monokulo.com/squares"))
    }

    @Test func aSignInEndsAtTheRedirectThePageNames() throws {
        let square = try #require(URL(string:
            "https://connect.squareup.com/oauth2/authorize?client_id=sq0idp-x&redirect_uri=https%3A%2F%2Ftill.monokulo.com%2Fsquare&state=s"))
        #expect(SignInCallback.of(page: square, fallback: "monokulo-till://square")
            == .https(host: "till.monokulo.com", path: "/square"))
        let scheme = try #require(URL(string: "https://example.com/oauth?redirect_uri=monokulo-till-debug%3A%2F%2Fclover"))
        #expect(SignInCallback.of(page: scheme, fallback: self.square) == .scheme("monokulo-till-debug"))
        // A page that names none ends at the setup's.
        let bare = try #require(URL(string: "https://example.com/oauth"))
        #expect(SignInCallback.of(page: bare, fallback: self.square) == .https(host: "till.monokulo.com", path: "/square"))
    }

    @Test func aSandboxSignInEndsAtTheDebugPageAndComesBackThroughTheScheme() throws {
        let host = "till.monokulo.com"
        let square = TillShared.squareRedirect(sandbox: true, host: host)
        let clover = TillShared.cloverRedirect(environment: .sandbox, host: host)
        #expect(square == "https://till.monokulo.com/square/debug/")
        #expect(clover == "https://till.monokulo.com/clover/debug/")
        #expect(TillShared.squareRedirect(sandbox: false, host: host) == self.square)
        #expect(TillShared.cloverRedirect(environment: .europe, host: host) == self.clover)
        // Without one, the build's (a test build's is project.yml's placeholder).
        #expect(TillShared.squareRedirect(sandbox: false) == "https://\(Build.linkHost)/square")
        #expect(Build.linkHost == "till.invalid")
        let page = try #require(URL(string: "https://example.com/oauth"))
        #expect(SignInCallback.of(page: page, fallback: square) == .https(host: "till.monokulo.com", path: "/square/debug/"))
        // The debug page's forward to the app reaches the engine as its redirect.
        let back = try #require(URL(string: "monokulo-till://square?code=c&state=s"))
        #expect(IncomingLink.of(back, square: square, clover: clover)
            == .signInReturned("https://till.monokulo.com/square/debug/?code=c&state=s"))
    }

    @Test func alertsCountAsAllowedWhenTheyReachTheCashier() {
        #expect(AlertsPermission.allows(.authorized))
        #expect(AlertsPermission.allows(.provisional))
        #expect(!AlertsPermission.allows(.denied))
        #expect(!AlertsPermission.allows(.notDetermined))
    }

    @Test func iPhonesBarIsHowFarSetupHasCome() {
        #expect(SetupProgress.fraction(StepProgress(value: 1, total: 4)) == 0.25)
        #expect(SetupProgress.fraction(StepProgress(value: 4, total: 4)) == 1)
        #expect(SetupProgress.fraction(StepProgress(value: 1, total: 0)) == 0)
    }

    @Test func welcomesFeaturesHaveTheirSymbols() {
        #expect(FeatureLook.symbol(.sales) == "bag")
        #expect(FeatureLook.symbol(.code) == "qrcode")
        #expect(FeatureLook.symbol(.confirmed) == "checkmark.circle")
    }

    @Test func boldRunsAreReadFromDoubleAsterisks() {
        #expect(Rich.runs("Open **Settings › Checkout** now") == [
            Rich.Run(text: "Open ", bold: false),
            Rich.Run(text: "Settings › Checkout", bold: true),
            Rich.Run(text: " now", bold: false),
        ])
        #expect(Rich.runs("Plain") == [Rich.Run(text: "Plain", bold: false)])
    }
}
