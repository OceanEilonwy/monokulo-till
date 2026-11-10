//! Clover's low-trust sign-in, PKCE on the device, against a stand-in
//! Clover. Ported test for test from the Android app's `CloverAuthTest.kt`.

#![cfg(feature = "clover")]

mod common;

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use common::*;
use monokulo_client::FixedClock;
use till_pos::clover::{CloverAuth, CloverConfig, CloverEnvironment};
use till_pos::{Pkce, PointOfSale, PosTokens};

struct Fixture {
    stand: Stand,
    tokens: Arc<MemoryTokens>,
    clock: FixedClock,
    config: CloverConfig,
    auth: CloverAuth,
}

impl Fixture {
    async fn start() -> Fixture {
        let stand = Stand::start().await;
        let tokens = Arc::new(MemoryTokens::default());
        let clock = FixedClock::at(at("2026-10-28T09:00:00Z"));
        let config = CloverConfig {
            user_agent: "Till/0.3.0".into(),
            base_url: Some(stand.url()),
            ..CloverConfig::new(
                "CLOVERAPP0001",
                CloverEnvironment::NorthAmerica,
                "https://till.monokulo.com/clover",
            )
        };
        let auth = CloverAuth::new(
            reqwest::Client::new(),
            config.clone(),
            tokens.clone(),
            Arc::new(clock.clone()),
        );
        Fixture {
            stand,
            tokens,
            clock,
            config,
            auth,
        }
    }
}

/// Clover's token answer: the expirations in Unix seconds.
fn token_answer(access: &str, refresh: &str, expires_at: i64) -> wiremock::ResponseTemplate {
    json(&format!(
        r#"{{"access_token":"{access}","access_token_expiration":{expires_at},"refresh_token":"{refresh}","refresh_token_expiration":1824714000}}"#
    ))
}

#[tokio::test]
async fn each_environment_has_its_own_sign_in_page_and_api_and_a_stand_in_replaces_both() {
    let f = Fixture::start().await;
    let sandbox = CloverConfig::new(
        "A",
        CloverEnvironment::Sandbox,
        "https://till.monokulo.com/clover",
    );
    assert_eq!(sandbox.web_base(), "https://sandbox.dev.clover.com");
    assert_eq!(sandbox.api_base(), "https://apisandbox.dev.clover.com");
    let europe = CloverConfig {
        environment: CloverEnvironment::Europe,
        ..sandbox.clone()
    };
    assert_eq!(europe.api_base(), "https://api.eu.clover.com");
    let latin_america = CloverConfig {
        environment: CloverEnvironment::LatinAmerica,
        ..sandbox.clone()
    };
    assert_eq!(latin_america.web_base(), "https://www.la.clover.com");
    assert_eq!(f.config.web_base(), f.config.api_base());
    assert_eq!(
        CloverEnvironment::from_wire("north_america"),
        Some(CloverEnvironment::NorthAmerica)
    );
    assert_eq!(
        CloverEnvironment::ALL.map(CloverEnvironment::wire),
        ["sandbox", "north_america", "europe", "latin_america"],
        "the Bell's environment names"
    );
    assert_eq!(CloverEnvironment::from_wire("production"), None);
}

#[tokio::test]
async fn the_sign_in_page_gets_the_pkce_challenge_the_redirect_and_a_state_and_no_secret() {
    let f = Fixture::start().await;
    let pkce = Pkce::create();
    let url = reqwest::Url::parse(&f.auth.authorize_url(&pkce)).unwrap();
    let param = |name: &str| {
        url.query_pairs()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.into_owned())
    };
    assert_eq!(url.path(), "/oauth/v2/authorize");
    assert_eq!(param("client_id").as_deref(), Some("CLOVERAPP0001"));
    assert_eq!(
        param("redirect_uri").as_deref(),
        Some("https://till.monokulo.com/clover")
    );
    assert_eq!(param("code_challenge"), Some(pkce.challenge()));
    assert_eq!(param("code_challenge_method").as_deref(), Some("S256"));
    assert_eq!(param("state"), Some(pkce.state.clone()));
    assert_eq!(
        url.query_pairs()
            .map(|(n, _)| n.into_owned())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(
            [
                "client_id",
                "redirect_uri",
                "code_challenge",
                "code_challenge_method",
                "state"
            ]
            .map(String::from)
        )
    );
}

#[tokio::test]
async fn finishing_posts_exactly_the_code_and_verifier_and_keeps_the_tokens_with_the_merchant_the_expiry_as_rfc_3339()
 {
    let f = Fixture::start().await;
    let pkce = Pkce::create();
    f.stand.enqueue(token_answer("acc1", "ref1", 1761600000));
    let signed_in = f
        .auth
        .finish(&pkce, Some("MCLOVER00001"), Some("code1"), None, None)
        .await
        .unwrap();
    assert_eq!(
        signed_in,
        tokens("acc1", "2025-10-27T21:20:00Z", "ref1", "MCLOVER00001"),
        "Unix seconds, never read as milliseconds"
    );
    assert_eq!(f.tokens.get(), Some(signed_in));
    assert!(f.auth.signed_in().unwrap());
    let request = f.stand.take_request().await;
    assert_eq!(request.url.path(), "/oauth/v2/token");
    assert_eq!(
        header(&request, "User-Agent").as_deref(),
        Some("Till/0.3.0")
    );
    let body = body_json(&request);
    assert_eq!(
        body.keys().map(String::as_str).collect::<BTreeSet<_>>(),
        BTreeSet::from(["client_id", "code", "code_verifier"]),
        "no secret, no grant type Clover doesn't ask for"
    );
    assert_eq!(body["code_verifier"], pkce.verifier.as_str());
    assert_eq!(body["code"], "code1");
}

#[tokio::test]
async fn an_echoed_state_must_be_the_attempts_and_a_redirect_needs_its_merchant_and_code() {
    let f = Fixture::start().await;
    let pkce = Pkce::create();
    let other = f
        .auth
        .finish(
            &pkce,
            Some("MCLOVER00001"),
            Some("code"),
            Some("other"),
            None,
        )
        .await
        .unwrap_err();
    assert_eq!(
        other.to_string(),
        "That sign-in wasn't the one this app started. Try again."
    );
    assert_eq!(other.pos(), Some(PointOfSale::Clover));
    for (merchant, code) in [
        (None, Some("code")),
        (Some("MCLOVER00001"), None),
        (Some("../tenders"), Some("code")),
    ] {
        let e = f
            .auth
            .finish(&pkce, merchant, code, None, None)
            .await
            .unwrap_err();
        assert_eq!(e.pos(), Some(PointOfSale::Clover), "{merchant:?} {code:?}");
    }
    let refused = f
        .auth
        .finish(&pkce, None, None, None, Some("access_denied"))
        .await
        .unwrap_err();
    assert_eq!(
        refused.to_string(),
        "Clover sign-in was cancelled or refused (access_denied)."
    );
    assert_eq!(f.stand.request_count().await, 0, "Clover isn't asked");
    assert!(!f.auth.signed_in().unwrap());

    f.stand.enqueue(token_answer("acc1", "ref1", 1793179800));
    f.auth
        .finish(
            &pkce,
            Some("MCLOVER00001"),
            Some("code"),
            Some(&pkce.state),
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        f.tokens.get().unwrap().expires_at.as_str(),
        "2026-10-28T09:30:00Z"
    );
}

#[tokio::test]
async fn a_token_with_under_five_minutes_left_is_refreshed_first_and_the_new_pair_kept() {
    let f = Fixture::start().await;
    f.tokens.set(Some(tokens(
        "old",
        "2026-10-28T09:04:59Z",
        "ref1",
        "MCLOVER00001",
    )));
    f.stand.enqueue(token_answer("new", "ref2", 1793179800));
    assert_eq!(f.auth.access_token().await.unwrap(), "new");
    assert_eq!(
        f.tokens.get(),
        Some(tokens(
            "new",
            "2026-10-28T09:30:00Z",
            "ref2",
            "MCLOVER00001"
        ))
    );
    let request = f.stand.take_request().await;
    assert_eq!(request.url.path(), "/oauth/v2/refresh");
    let body = body_json(&request);
    assert_eq!(
        body.keys().map(String::as_str).collect::<BTreeSet<_>>(),
        BTreeSet::from(["client_id", "refresh_token"])
    );
    assert_eq!(body["refresh_token"], "ref1");
    // 25 minutes left now: no request.
    f.clock.advance(60);
    assert_eq!(f.auth.access_token().await.unwrap(), "new");
    assert_eq!(f.stand.request_count().await, 1);
}

#[tokio::test]
async fn a_refused_refresh_signs_out_and_says_to_sign_in_again_once() {
    let f = Fixture::start().await;
    f.tokens.set(Some(tokens(
        "old",
        "2026-10-28T09:01:00Z",
        "ref1",
        "MCLOVER00001",
    )));
    f.stand
        .enqueue(status(401, r#"{"message":"Invalid refresh token"}"#));
    let e = f.auth.access_token().await.unwrap_err();
    assert!(e.sign_in_again());
    assert_eq!(e.to_string(), "Clover's sign-in has ended. Sign in again.");
    assert_eq!(f.tokens.get(), None);
    assert!(!f.auth.signed_in().unwrap());
    let again = f.auth.access_token().await.unwrap_err();
    assert!(again.sign_in_again());
    assert_eq!(
        f.stand.request_count().await,
        1,
        "no second refresh with a spent token"
    );
}

#[tokio::test]
async fn a_busy_or_failing_clover_leaves_the_sign_in_for_the_next_try() {
    let f = Fixture::start().await;
    let kept = tokens("old", "2026-10-28T09:01:00Z", "ref1", "MCLOVER00001");
    f.tokens.set(Some(kept.clone()));
    f.stand
        .enqueue(status(503, r#"{"message":"Service unavailable"}"#));
    let down = f.auth.access_token().await.unwrap_err();
    assert!(!down.sign_in_again());
    assert_eq!(down.to_string(), "Service unavailable");
    assert_eq!(f.tokens.get(), Some(kept.clone()));
    f.stand
        .enqueue(wiremock::ResponseTemplate::new(429).insert_header("Retry-After", "3"));
    let busy = f.auth.access_token().await.unwrap_err();
    assert!(!busy.sign_in_again(), "a 429 is a 4xx that isn't a refusal");
    assert_eq!(busy.retry_after_seconds(), Some(3));
    assert_eq!(f.tokens.get(), Some(kept));
}

#[tokio::test]
async fn signing_out_forgets_the_tokens() {
    let f = Fixture::start().await;
    f.tokens.set(Some(tokens(
        "acc",
        "2026-10-28T09:30:00Z",
        "ref",
        "MCLOVER00001",
    )));
    f.auth.sign_out();
    assert_eq!(f.tokens.get(), None);
}

// Not in CloverAuthTest.kt: the lock docs/ios-plan.md §5 asks for.

#[tokio::test]
async fn two_refreshes_after_one_refusal_sharing_a_lock_make_one_refresh_request() {
    let f = Fixture::start().await;
    f.tokens.set(Some(tokens(
        "acc1",
        "2026-10-28T09:30:00Z",
        "ref1",
        "MCLOVER00001",
    )));
    f.stand
        .enqueue(token_answer("acc2", "ref2", 1793179800).set_delay(Duration::from_millis(200)));
    let lock = Arc::new(SharedLock::default());
    let sign_in = || {
        CloverAuth::new(
            reqwest::Client::new(),
            f.config.clone(),
            f.tokens.clone(),
            Arc::new(f.clock.clone()),
        )
        .with_lock(lock.clone())
    };
    let (app, extension) = (sign_in(), sign_in());
    // Both had acc1 refused at once.
    let (a, b) = tokio::join!(
        app.refresh_after_refusal("acc1"),
        extension.refresh_after_refusal("acc1")
    );
    assert_eq!((a.unwrap(), b.unwrap()), ("acc2".into(), "acc2".into()));
    assert_eq!(f.stand.request_count().await, 1, "ref1 spent once");
    let kept: PosTokens = f.tokens.get().unwrap();
    assert_eq!(kept.refresh_token.as_deref(), Some("ref2"));
}
