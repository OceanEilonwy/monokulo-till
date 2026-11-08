//! Tor, as the app reaches it (Android: `core/Tor.kt`): through a Tor app on
//! the device (Orbot) and its SOCKS proxy, `127.0.0.1:9050`
//! (docs/decisions/android.md 55). Monokular has no Tor of its own, and iOS
//! has none at all (docs/ios-plan.md §11).

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;

use reqwest::Url;

/// Orbot's SOCKS proxy, where it listens unless told otherwise.
pub const SOCKS: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 9050));

/// Whether `url` is an http or https address on an onion host: only Tor
/// reaches it. Anything that isn't a web address is not.
pub fn is_onion(url: &str) -> bool {
    Url::parse(url).is_ok_and(|url| onion_host(&url))
}

fn onion_host(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
        && url
            .host_str()
            .is_some_and(|host| host.to_ascii_lowercase().ends_with(".onion"))
}

/// Where a request to `url` goes first: through Tor's SOCKS proxy at
/// `socks` for an onion address, as `socks5h` so the proxy looks the name up
/// itself; `None`, straight out, for everything else.
pub fn proxy_for(url: &Url, socks: SocketAddr) -> Option<Url> {
    if onion_host(url) {
        Url::parse(&format!("socks5h://{socks}")).ok()
    } else {
        None
    }
}

/// [`proxy_for`] as a reqwest proxy.
pub fn proxy(socks: SocketAddr) -> reqwest::Proxy {
    reqwest::Proxy::custom(move |url| proxy_for(url, socks))
}

/// The HTTP client the app reaches a store through (Android:
/// `MonokularApp.http`): onion addresses through Tor's proxy at `socks`, the
/// rest straight out; 30 seconds to connect, since Tor takes longer to build
/// a circuit; and 60 seconds of silence on a read before the connection
/// counts as dead, as the event stream sends a comment every 25 seconds.
pub fn http_client(socks: SocketAddr) -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .proxy(proxy(socks))
        .connect_timeout(Duration::from_secs(30))
        .read_timeout(Duration::from_secs(60))
        .build()
}

/// Whether Tor is running on the device, so a till can go over it: on
/// Android, whether something listens on Orbot's SOCKS port. The probe is
/// the platform's (it stays in Kotlin on Android, and iOS has no Tor), so
/// the core asks through this trait; a platform without Tor answers
/// `false`.
pub trait TorProbe: Send + Sync {
    fn running(&self) -> bool;
}
