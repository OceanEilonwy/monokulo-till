//! Clover's sign-in (Android: `CloverAuth.kt`).

use std::sync::Arc;

use monokulo_client::{Clock, PointOfSale};
use serde::Deserialize;
use serde_json::json;
use till_types::Secret;

use super::{CloverConfig, CloverMerchantId, clover_error, retry_after};
use crate::http;
use crate::tokens::{hold, no_lock};
use crate::{OAuthRedirect, Pkce, PosError, PosTokens, StoreError, TokenLock, TokenStore};

/// Clover's token answer: the expirations are Unix seconds, where the rest
/// of Clover is milliseconds.
#[derive(Deserialize)]
struct TokenAnswer {
    access_token: Secret,
    #[serde(default)]
    access_token_expiration: Option<i64>,
    #[serde(default)]
    refresh_token: Option<Secret>,
}

/// Clover's v2 OAuth for a low-trust app: the authorization code with
/// PKCE, on the device, and no secret anywhere (decision 127 in
/// docs/decisions/android.md).
///
/// Access tokens last about 30 minutes and are refreshed when under
/// [`REFRESH_BEFORE_SECONDS`](Self::REFRESH_BEFORE_SECONDS) is left. A
/// refresh token is single-use, so the new pair is kept before anything
/// else, and Clover caps how many a merchant's app may hold, so a refused
/// refresh means signing in again (decision 128).
pub struct CloverAuth {
    http: reqwest::Client,
    config: CloverConfig,
    store: Arc<dyn TokenStore>,
    clock: Arc<dyn Clock>,
    lock: Arc<dyn TokenLock>,
    /// This process's own turn at the sign-in, taken before `lock`.
    turn: tokio::sync::Mutex<()>,
}

impl CloverAuth {
    /// An access token with less than this left is refreshed first: five
    /// minutes.
    pub const REFRESH_BEFORE_SECONDS: i64 = 5 * 60;

    /// How long an access token lasts when Clover doesn't say.
    const LIFETIME_SECONDS: i64 = 30 * 60;

    /// A sign-in kept in `store`, refreshed by its owner alone
    /// ([`NoTokenLock`](crate::NoTokenLock)).
    pub fn new(
        http: reqwest::Client,
        config: CloverConfig,
        store: Arc<dyn TokenStore>,
        clock: Arc<dyn Clock>,
    ) -> CloverAuth {
        CloverAuth {
            http,
            config,
            store,
            clock,
            lock: no_lock(),
            turn: tokio::sync::Mutex::new(()),
        }
    }

    /// The same sign-in, with `lock` held around every refresh and every
    /// new sign-in kept, for a store another process shares.
    pub fn with_lock(mut self, lock: Arc<dyn TokenLock>) -> CloverAuth {
        self.lock = lock;
        self
    }

    /// The page the merchant signs in on. Clover documents no `state`: one
    /// is sent all the same, and [`finish`](Self::finish) checks it when
    /// Clover sends it back.
    pub fn authorize_url(&self, pkce: &Pkce) -> String {
        let challenge = pkce.challenge();
        http::url(
            self.config.web_base(),
            &["oauth", "v2", "authorize"],
            &[
                ("client_id", &self.config.application_id),
                ("redirect_uri", &self.config.redirect_uri),
                ("code_challenge", &challenge),
                ("code_challenge_method", "S256"),
                ("state", pkce.state.as_str()),
            ],
        )
    }

    /// Whether a sign-in is kept; a store that can't tell says so.
    pub fn signed_in(&self) -> Result<bool, StoreError> {
        Ok(self.store.load()?.is_some())
    }

    /// Clover's redirect, back in the app: the code for this attempt's
    /// `pkce` at the merchant the cashier chose. `merchant_id` and `code`
    /// are needed; `state` must be the attempt's when Clover echoes one;
    /// `error` ends it. The tokens are kept with the merchant before
    /// they're answered.
    pub async fn finish(
        &self,
        pkce: &Pkce,
        redirect: &OAuthRedirect,
    ) -> Result<PosTokens, PosError> {
        let refused = |message: String| PosError::answered(PointOfSale::Clover, 0, message);
        if let Some(error) = &redirect.error {
            return Err(refused(format!(
                "Clover sign-in was cancelled or refused ({error})."
            )));
        }
        if redirect
            .state
            .as_ref()
            .is_some_and(|state| *state != pkce.state)
        {
            return Err(refused(
                "That sign-in wasn't the one this app started. Try again.".into(),
            ));
        }
        let (Some(code), Some(merchant_id)) = (redirect.code(), redirect.merchant_id.as_deref())
        else {
            return Err(refused(
                "Clover didn't say which merchant signed in. Try again.".into(),
            ));
        };
        let Some(merchant_id) = CloverMerchantId::new(merchant_id) else {
            return Err(refused(
                "Clover didn't say which merchant signed in. Try again.".into(),
            ));
        };
        let answer = self
            .token(
                "oauth/v2/token",
                json!({
                    "client_id": self.config.application_id,
                    "code": code.0,
                    "code_verifier": pkce.verifier,
                }),
            )
            .await?;
        let tokens = self.tokens(answer, merchant_id.as_str());
        let _turn = self.turn.lock().await;
        let _held = hold(&self.lock).await?;
        self.store.save(Some(tokens.clone()));
        Ok(tokens)
    }

    /// An access token with at least
    /// [`REFRESH_BEFORE_SECONDS`](Self::REFRESH_BEFORE_SECONDS) left,
    /// refreshed first when it hasn't. A refresh Clover refuses (a `4xx`
    /// other than `429`) signs out and says to sign in again; a busy or
    /// unreachable Clover leaves the sign-in as it was, for the next try.
    pub async fn access_token(&self) -> Result<Secret, PosError> {
        let _turn = self.turn.lock().await;
        let tokens = self.signed_in_tokens()?;
        if self.lasts(&tokens) {
            return Ok(tokens.access_token);
        }
        let _held = hold(&self.lock).await?;
        // Another process sharing the sign-in may have refreshed it while
        // this one waited for the lock.
        let tokens = self.signed_in_tokens()?;
        if self.lasts(&tokens) {
            return Ok(tokens.access_token);
        }
        self.refresh(tokens).await
    }

    /// Clover refused `refused` before its time (it ended early, or the
    /// merchant's tokens were reset): a new one, refreshed now, unless
    /// another request (or process) already did. The same rules as
    /// [`access_token`](Self::access_token) (decision 139).
    pub async fn refresh_after_refusal(&self, refused: &Secret) -> Result<Secret, PosError> {
        let _turn = self.turn.lock().await;
        let tokens = self.signed_in_tokens()?;
        if tokens.access_token != *refused {
            return Ok(tokens.access_token);
        }
        let _held = hold(&self.lock).await?;
        let tokens = self.signed_in_tokens()?;
        if tokens.access_token != *refused {
            return Ok(tokens.access_token);
        }
        self.refresh(tokens).await
    }

    /// Forgets the sign-in.
    pub fn sign_out(&self) {
        self.store.save(None);
    }

    fn signed_in_tokens(&self) -> Result<PosTokens, PosError> {
        self.store
            .load()?
            .ok_or_else(|| PosError::sign_in(PointOfSale::Clover, 401, "Sign in to Clover."))
    }

    /// Whether `tokens`' access token has more than five minutes left.
    fn lasts(&self, tokens: &PosTokens) -> bool {
        let due = crate::time::from_seconds(
            self.clock
                .now()
                .saturating_add(Self::REFRESH_BEFORE_SECONDS),
        );
        tokens.expires_at.timestamp() > due
    }

    /// Spends `tokens`' refresh token on a new pair, kept before it's
    /// answered. Called with the lock held.
    async fn refresh(&self, tokens: PosTokens) -> Result<Secret, PosError> {
        let Some(refresh) = tokens.refresh_token else {
            return Err(self.signed_out(401));
        };
        let renewed = match self
            .token(
                "oauth/v2/refresh",
                json!({"client_id": self.config.application_id, "refresh_token": refresh}),
            )
            .await
        {
            Ok(renewed) => renewed,
            Err(e) if e.is_refusal() => return Err(self.signed_out(e.status().unwrap_or(0))),
            Err(e) => return Err(e),
        };
        // The old refresh token is spent: keep the new one first.
        let saved = self.tokens(renewed, &tokens.merchant_id);
        self.store.save(Some(saved.clone()));
        Ok(saved.access_token)
    }

    /// Forgets the sign-in, once, and says so.
    fn signed_out(&self, status: u16) -> PosError {
        self.store.save(None);
        PosError::sign_in(
            PointOfSale::Clover,
            status,
            "Clover's sign-in has ended. Sign in again.",
        )
    }

    /// `answer` as the sign-in kept: its expiry in seconds, never
    /// milliseconds read as seconds, and 30 minutes from now when Clover
    /// says nothing.
    fn tokens(&self, answer: TokenAnswer, merchant_id: &str) -> PosTokens {
        let expires = answer
            .access_token_expiration
            .unwrap_or_else(|| self.clock.now().saturating_add(Self::LIFETIME_SECONDS));
        PosTokens {
            access_token: answer.access_token,
            expires_at: crate::time::Rfc3339Instant::from_second(expires),
            refresh_token: answer.refresh_token,
            merchant_id: merchant_id.to_owned(),
        }
    }

    async fn token(&self, path: &str, body: serde_json::Value) -> Result<TokenAnswer, PosError> {
        let request = self
            .http
            .post(format!("{}/{path}", self.config.api_base()))
            .header(reqwest::header::USER_AGENT, &self.config.user_agent)
            .header(reqwest::header::ACCEPT, "application/json")
            .json(&body);
        let answer = http::send(request).await?;
        if !answer.ok() {
            return Err(PosError::Answered {
                pos: PointOfSale::Clover,
                status: answer.status,
                message: clover_error(&answer.body)
                    .unwrap_or_else(|| format!("Clover said {}.", answer.status)),
                sign_in_again: false,
                retry_after_seconds: retry_after(answer.retry_after.as_deref()),
            });
        }
        serde_json::from_str(&answer.body).map_err(|_| {
            PosError::answered(
                PointOfSale::Clover,
                answer.status,
                "Clover's sign-in answer couldn't be read.",
            )
        })
    }
}
