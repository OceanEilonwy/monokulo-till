import AuthenticationServices
import UIKit

/// A point of sale's sign-in page (`Effect.openUrl`), opened in an
/// `ASWebAuthenticationSession` (docs/ios-plan.md §5): the core built the
/// page's address with PKCE and exchanges the code; this only shows the page
/// and hands back where it was sent.
@MainActor
final class SignInSession: NSObject, @preconcurrency ASWebAuthenticationPresentationContextProviding {
    private var session: ASWebAuthenticationSession?

    /// Opens `page`. `returned` gets the redirect with its code, or nil when
    /// the cashier closed the page or it failed.
    func start(_ page: URL, fallback: String, returned: @escaping @MainActor @Sendable (URL?) -> Void) {
        session?.cancel()
        guard let ends = SignInCallback.of(page: page, fallback: fallback) else {
            // No redirect to wait for: the browser, and the universal link back.
            UIApplication.shared.open(page)
            return
        }
        let callback: ASWebAuthenticationSession.Callback
        switch ends {
        case .https(let host, let path): callback = .https(host: host, path: path)
        case .scheme(let scheme): callback = .customScheme(scheme)
        }
        let session = ASWebAuthenticationSession(url: page, callback: callback) { @Sendable url, _ in
            Task { @MainActor in returned(url) }
        }
        session.presentationContextProvider = self
        // The browser's own sign-in to Square or Clover helps the merchant.
        session.prefersEphemeralWebBrowserSession = false
        self.session = session
        if !session.start() {
            self.session = nil
            returned(nil)
        }
    }

    func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        let windows = scenes.flatMap(\.windows)
        if let window = windows.first(where: \.isKeyWindow) ?? windows.first {
            return window
        }
        // A sign-in only starts from a screen on show, so there is a scene.
        return UIWindow(windowScene: scenes[0])
    }
}
