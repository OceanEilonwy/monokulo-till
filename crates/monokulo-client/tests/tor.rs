//! Ported from the Android app's `TorTest`. Its third test, that Tor is
//! running when something listens on its proxy port, stays in Kotlin with the
//! probe it tests (`tor::TorProbe` is the seam).

use std::net::SocketAddr;

use monokulo_client::tor;
use reqwest::Url;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;

#[test]
fn onion_addresses_go_through_tors_socks_proxy_and_nothing_else_does() {
    let onion =
        Url::parse("http://2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion/api/v1")
            .unwrap();
    let proxy = tor::proxy_for(&onion, tor::SOCKS).unwrap();
    assert_eq!(
        proxy.as_str(),
        "socks5h://127.0.0.1:9050",
        "socks5h: the proxy looks the name up itself"
    );
    let web = Url::parse("https://pay.example/api/v1").unwrap();
    assert_eq!(tor::proxy_for(&web, tor::SOCKS), None);
}

#[test]
fn only_an_onion_host_is_an_onion_address() {
    assert!(tor::is_onion("http://abc.onion/api/v1"));
    assert!(tor::is_onion("http://ABC.ONION:8080/x"));
    assert!(!tor::is_onion("https://onion.example/api/v1"));
    assert!(!tor::is_onion("not a url"));
}

/// Not in the Kotlin: the app's client really does send an onion address to
/// the SOCKS proxy, and a web address past it.
#[tokio::test]
async fn the_apps_client_sends_an_onion_address_to_the_socks_proxy() {
    let socks = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let at: SocketAddr = socks.local_addr().unwrap();
    let http = tor::http_client(at).unwrap();
    let request = tokio::spawn(async move { http.get("http://abc.onion/api/v1").send().await });
    let (mut connection, _) = socks.accept().await.unwrap();
    let mut greeting = [0u8; 1];
    connection.read_exact(&mut greeting).await.unwrap();
    assert_eq!(greeting[0], 5, "a SOCKS5 greeting");
    drop(connection);
    assert!(request.await.unwrap().is_err());

    let web = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = web.local_addr().unwrap().port();
    let http = tor::http_client(at).unwrap();
    let request =
        tokio::spawn(async move { http.get(format!("http://127.0.0.1:{port}/")).send().await });
    let (mut connection, _) = web.accept().await.unwrap();
    let mut start = [0u8; 3];
    connection.read_exact(&mut start).await.unwrap();
    assert_eq!(&start, b"GET", "straight to the web address");
    drop(connection);
    let _ = request.await;
}
