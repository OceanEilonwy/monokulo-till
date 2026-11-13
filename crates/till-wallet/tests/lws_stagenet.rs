//! The light wallet server mode against a real monokulo-lws on stagenet
//! (docs/view-key-mode.md §5.7, §12 step 11). Ignored: it needs the server
//! running, and the server needs a stagenet node.
//!
//! In monokulo-lws's checkout:
//!
//! ```sh
//! cargo run --release -- serve --daemon http://stagenet.xmr-tw.org:38081 \
//!     --db /tmp/lws-till.sqlite --listen 127.0.0.1:8443 --network stage --create auto
//! ```
//!
//! Then here:
//!
//! ```sh
//! cargo test -p till-wallet --test lws_stagenet -- --ignored --nocapture
//! ```
//!
//! `TILL_LWS_URL` names another server (default `http://127.0.0.1:8443`).
//! With `TILL_LWS_WAIT_SECS=900` the test prints the payment's address and
//! amount and waits that long for someone to pay it from a stagenet wallet
//! (Feather, Cake, monero-wallet-cli), then checks it's seen.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use monokulo_client::{SalePhase, SystemClock};
use till_payments::{PaymentBackend, PaymentRequest};
use till_types::{CurrencyCode, Money};
use till_wallet::prices::PriceSource;
use till_wallet::{Network, WalletBackend, WalletSetup, check_light_server, make_wallet};

/// One XMR at 160 of anything: no price provider is asked.
struct Fixed;

#[async_trait]
impl PriceSource for Fixed {
    fn name(&self) -> String {
        "Fixed".into()
    }
    fn prices(&self, _: &str) -> bool {
        true
    }
    async fn current(&self, _: &str) -> Result<u64, String> {
        Ok(160_000_000)
    }
}

#[tokio::test]
#[ignore = "needs monokulo-lws running on stagenet (see the file's comment)"]
async fn a_till_registers_with_monokulo_lws_and_watches_its_account() {
    let url = std::env::var("TILL_LWS_URL").unwrap_or_else(|_| "http://127.0.0.1:8443".into());
    let check = check_light_server(Network::Mainnet, &url, false).await;
    assert!(check.passed(), "the server's checks: {check:?}");
    assert_eq!(check.network, Some(Network::Stagenet));

    let dir = tempfile::tempdir().unwrap();
    let keys = fob_keys::Keys::generate(Network::Stagenet).keys;
    let view = till_wallet::keys::keys_from_typed(
        Network::Stagenet,
        &keys.primary_address(),
        &keys.view_key_hex(),
    )
    .unwrap();
    let setup = WalletSetup {
        light_server: Some(url.clone()),
        ..WalletSetup::new(2)
    };
    let made = make_wallet(dir.path(), &view, &setup).await.unwrap();
    let link = made.link.clone();
    drop(made);
    let backend =
        WalletBackend::open_with(dir.path(), &link, Arc::new(SystemClock), Arc::new(Fixed))
            .await
            .unwrap();

    let payment = backend
        .create(PaymentRequest {
            key: "e2e-1".into(),
            amount: Money::new(160, CurrencyCode::USD),
            reference: None,
        })
        .await
        .unwrap();
    // Registered and told of the account: the refresh polls the server.
    let read = backend
        .refresh(std::slice::from_ref(&payment.reference))
        .await
        .unwrap();
    assert_eq!(read[0].phase, SalePhase::Waiting);
    assert!(
        backend.warnings().await.is_empty(),
        "approved at once with --create auto: {:?}",
        backend.warnings().await
    );

    let Some(wait) = std::env::var("TILL_LWS_WAIT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
    else {
        return;
    };
    println!(
        "Pay {} XMR (exactly: {} piconero) to {}",
        payment.xmr_piconero as f64 / 1e12,
        payment.xmr_piconero,
        payment.address
    );
    let deadline = tokio::time::Instant::now() + Duration::from_secs(wait);
    loop {
        let now = backend
            .refresh(std::slice::from_ref(&payment.reference))
            .await
            .unwrap();
        println!("{:?}, {} confirmations", now[0].phase, now[0].confirmations);
        if now[0].phase != SalePhase::Waiting {
            assert!(matches!(
                now[0].phase,
                SalePhase::Seen | SalePhase::Confirming | SalePhase::Paid
            ));
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "nothing seen in {wait} s"
        );
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}
