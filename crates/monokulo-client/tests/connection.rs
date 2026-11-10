//! Ported from the Android app's `ConnectionTest`: the connection as the app
//! keeps it on the device.

use monokulo_client::{Connection, Store};

const ONION: &str = "http://2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion/api/v1";

#[test]
fn a_connection_is_kept_as_its_key_and_the_apis_entry_nothing_about_the_store() {
    let connection = Connection::new("ak_x", "https://pay.example/api/v1");
    let saved = Connection::save(&connection);
    assert_eq!(
        saved,
        r#"{"app_key":"ak_x","api":"https://pay.example/api/v1"}"#
    );
    assert_eq!(Connection::restore(&saved), Some(connection));
}

#[test]
fn a_connection_never_prints_its_key() {
    let connection = Connection::new("ak_secret", "https://pay.example/api/v1");
    let printed = format!("{connection:?}");
    assert!(!printed.contains("ak_secret"), "{printed}");
    assert!(printed.contains("https://pay.example/api/v1"), "{printed}");
}

#[test]
fn a_connection_kept_before_monokulo_gave_onion_addresses_carries_on_off_tor() {
    let kept =
        Connection::restore(r#"{"app_key":"ak_x","api":"https://pay.example/api/v1"}"#).unwrap();
    assert_eq!(kept.onion_api, None);
    assert!(!kept.tor);
    assert_eq!(kept.entry(), "https://pay.example/api/v1");
}

#[test]
fn the_onion_entry_and_the_choice_of_tor_are_kept_with_it() {
    let connection = Connection {
        onion_api: Some(ONION.into()),
        tor: true,
        ..Connection::new("ak_x", "https://pay.example/api/v1")
    };
    assert_eq!(
        Connection::restore(&Connection::save(&connection)).as_ref(),
        Some(&connection)
    );
    assert_eq!(connection.entry(), ONION);
    assert!(connection.over_tor());
    assert!(
        !Connection {
            tor: false,
            ..connection
        }
        .over_tor()
    );
    assert!(
        Connection::new("ak_x", ONION).over_tor(),
        "connected at the onion address"
    );
}

#[test]
fn nothing_usable_restores_nothing() {
    assert_eq!(Connection::restore("not json"), None);
    assert_eq!(
        Connection::restore(r#"{"api":"https://pay.example/api/v1"}"#),
        None
    );
    assert_eq!(Connection::restore(r#"{"app_key":"ak_x"}"#), None);
}

#[test]
fn the_host_and_whether_its_encrypted_come_from_the_apis_address() {
    let tls = Connection::new("ak_x", "https://pay.example/api/v1");
    assert_eq!(tls.host(), "pay.example");
    assert!(tls.encrypted());
    let local = Connection::new("ak_x", "http://10.0.2.2:8080/api/v1");
    assert_eq!(local.host(), "10.0.2.2:8080");
    assert!(!local.encrypted());
}

#[test]
fn a_wallet_address_is_shortened_on_the_device_to_its_first_and_last_8() {
    assert_eq!(
        Store::shorten(
            "4AdUndXHHZ6cfufTMvppY6JwXNouMBzSkbLYfpAV5Usx3skxNgYeYTRj5UzqtReoS44qo9mtmXCqY45DJ852K5Jv2bNDXhdS7s"
        ),
        "4AdUndXH…NDXhdS7s"
    );
    assert_eq!(Store::shorten("short"), "short");
}
