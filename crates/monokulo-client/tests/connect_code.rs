//! Ported from the Android app's `ConnectCodeTest`.

use monokulo_client::ConnectCode;

fn token() -> String {
    format!("ct_{}", "0123456789abcdef".repeat(4))
}

#[test]
fn a_code_from_the_dashboard_gives_the_store_and_the_token() {
    let token = token();
    let code = ConnectCode::parse(
        &format!("https://pay.yourshop.com/apps/connect#token={token}"),
        false,
    )
    .unwrap();
    assert_eq!(code.connect_url, "https://pay.yourshop.com/apps/connect");
    assert_eq!(code.host(), "pay.yourshop.com");
    assert_eq!(code.token, token);
    // Pasted with spaces, a port, and the host's case.
    let pasted = ConnectCode::parse(
        &format!("  https://Pay.Example:8443/apps/connect#token={token} \n"),
        false,
    )
    .unwrap();
    assert_eq!(pasted.connect_url, "https://pay.example:8443/apps/connect");
}

#[test]
fn a_monokulo_under_a_path_keeps_it() {
    let code = ConnectCode::parse(
        &format!(
            "https://example.com/monokulo/apps/connect#token={}",
            token()
        ),
        false,
    )
    .unwrap();
    assert_eq!(
        code.connect_url,
        "https://example.com/monokulo/apps/connect"
    );
    assert_eq!(code.host(), "example.com");
}

#[test]
fn anything_else_is_not_a_code() {
    let token = token();
    for text in [
        "https://pay.example/apps/connect".to_owned(),
        "https://pay.example/apps/connect#token=ct_short".to_owned(),
        format!("https://pay.example/connect/monokular#token={token}"),
        format!("https://pay.example/myapps/connect#token={token}"),
        format!("https://pay.example/apps/connect/x#token={token}"),
        format!("https://pay.example/apps/connect?x=1#token={token}"),
        format!("https://user@pay.example/apps/connect#token={token}"),
        format!("ftp://pay.example/apps/connect#token={token}"),
        "monero:4Abc?tx_amount=1".to_owned(),
        "not a link".to_owned(),
    ] {
        assert_eq!(ConnectCode::parse(&text, false), None, "{text}");
    }
}

#[test]
fn plain_http_is_for_onion_addresses_or_a_debug_builds_own_machine() {
    let token = token();
    let onion = format!(
        "http://abcdefghijklmnopqrstuvwxyz234567abcdefghijklmnopqrstuvw.onion/apps/connect#token={token}"
    );
    assert_eq!(
        ConnectCode::parse(&onion, false).map(|c| c.token),
        Some(token.clone())
    );
    assert!(
        ConnectCode::parse(&onion, false).unwrap().encrypted(),
        "Tor encrypts the way to an onion address"
    );
    let local = format!("http://10.0.2.2:8080/apps/connect#token={token}");
    assert_eq!(ConnectCode::parse(&local, false), None);
    assert_eq!(
        ConnectCode::parse(&local, true).map(|c| c.connect_url),
        Some("http://10.0.2.2:8080/apps/connect".to_owned())
    );
    assert!(!ConnectCode::parse(&local, true).unwrap().encrypted());
}
