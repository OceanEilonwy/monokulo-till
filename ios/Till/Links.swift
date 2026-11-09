import Foundation

/// A link that opened the app (`.onOpenURL`: a universal link on the
/// build's link host, or the `monokulo-till` scheme), as the engine takes it.
///
/// - A point of sale's redirect, `https://<link host>/square?code=…`
///   (or `/clover`), is a sign-in coming back. So is the scheme's
///   `monokulo-till://square?…`, rewritten to the redirect the engine's
///   setup names, since the engine matches its `redirect_uri`.
/// - "Open in Till", `https://<link host>/connect#<connect link>`,
///   carries the store's connect link percent-encoded in its fragment, which
///   never leaves the device, as the link's own `#token=` never does.
/// - Anything else is handed over as a connect link, for the engine to take
///   or refuse.
enum IncomingLink: Equatable {
    case connect(String)
    case signInReturned(String)

    /// Till's own web host, where its universal links live: the build's
    /// `TILL_LINK_HOST`.
    static var host: String { Build.linkHost }
    /// The app's URL schemes: release, and debug builds'.
    static let schemes: Set<String> = ["monokulo-till", "monokulo-till-debug"]

    /// `url` read against the setups' redirect URIs (`SquareSetup` and
    /// `CloverSetup`'s `redirectUri`).
    static func of(_ url: URL, square: String, clover: String, host linkHost: String = IncomingLink.host) -> IncomingLink {
        let text = url.absoluteString
        let opened = Self.base(text)
        if opened == Self.base(square) || opened == Self.base(clover) {
            return .signInReturned(text)
        }
        let parts = URLComponents(url: url, resolvingAgainstBaseURL: false)
        let scheme = parts?.scheme?.lowercased() ?? ""
        let host = parts?.host?.lowercased() ?? ""
        if schemes.contains(scheme) {
            switch host {
            case "square": return .signInReturned(Self.base(square) + query(text))
            case "clover": return .signInReturned(Self.base(clover) + query(text))
            default: break
            }
        }
        if scheme == "https", host == linkHost.lowercased(), parts?.path.hasPrefix("/connect") == true {
            let wrapped = parts?.percentEncodedFragment?.removingPercentEncoding ?? ""
            return .connect(wrapped.isEmpty ? text : wrapped)
        }
        return .connect(text)
    }

    /// A URI without its query and fragment, as the engine compares redirects.
    static func base(_ uri: String) -> String {
        String(uri.prefix { $0 != "?" && $0 != "#" })
    }

    /// The query and fragment, from the `?` on; empty when there's none.
    private static func query(_ uri: String) -> String {
        guard let start = uri.firstIndex(of: "?") else { return "" }
        return String(uri[start...])
    }
}

/// Where a point of sale's sign-in page sends the browser back, which ends
/// the `ASWebAuthenticationSession`: the `redirect_uri` the engine wrote
/// into the page's address (else the setup's), as an https path on a host
/// (needing the app's `webcredentials` domain) or a custom scheme.
enum SignInCallback: Equatable {
    case https(host: String, path: String)
    case scheme(String)

    static func of(page: URL, fallback: String) -> SignInCallback? {
        let named = URLComponents(url: page, resolvingAgainstBaseURL: false)?
            .queryItems?.first { $0.name == "redirect_uri" }?.value
        guard let redirect = URLComponents(string: named ?? fallback),
              let scheme = redirect.scheme?.lowercased()
        else { return nil }
        guard scheme == "https" else { return .scheme(scheme) }
        guard let host = redirect.host, !host.isEmpty else { return nil }
        return .https(host: host, path: redirect.path.isEmpty ? "/" : redirect.path)
    }
}
