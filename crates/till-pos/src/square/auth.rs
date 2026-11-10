//! Square's sign-in (Android: `SquareAuth.kt`).

use std::sync::Arc;

use monokulo_client::{Clock, PointOfSale};
use serde_json::json;

use super::{SquareConfig, answered};
use crate::http;
use crate::tokens::{hold, no_lock};
use crate::{Pkce, PosError, PosTokens, StoreError, TokenLock, TokenStore};

/// An access token with less than this left is refreshed first: a day
/// (decision 19).
const REFRESH_BEFORE_SECONDS: i64 = 24 * 60 * 60;

/// Square OAuth with PKCE, on the device (docs/design.md §1): no secret
/// anywhere. PKCE refresh tokens are single-use and last 90 days, so each
/// refresh keeps the new one before anything else, and a lapsed one means
/// signing in again.
pub struct SquareAuth {
    http: reqwest::Client,
    config: SquareConfig,
    store: Arc<dyn TokenStore>,
    clock: Arc<dyn Clock>,
    lock: Arc<dyn TokenLock>,
    /// This process's own turn at the sign-in, taken before `lock`.
    turn: tokio::sync::Mutex<()>,
}

impl SquareAuth {
    /// A sign-in kept in `store`, refreshed by its owner alone
    /// ([`NoTokenLock`](crate::NoTokenLock)).
    pub fn new(
        http: reqwest::Client,
        config: SquareConfig,
        store: Arc<dyn TokenStore>,
        clock: Arc<dyn Clock>,
    ) -> SquareAuth {
        SquareAuth {
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
    pub fn with_lock(mut self, lock: Arc<dyn TokenLock>) -> SquareAuth {
        self.lock = lock;
        self
    }

    /// The page the cashier signs in on.
    pub fn authorize_url(&self, pkce: &Pkce) -> String {
        let scopes = SquareConfig::SCOPES.join(" ");
        let challenge = pkce.challenge();
        let mut query = vec![
            ("client_id", self.config.application_id.as_str()),
            ("scope", scopes.as_str()),
        ];
        // Signs the seller in afresh each time; the sandbox refuses it.
        if !self.config.sandbox {
            query.push(("session", "false"));
        }
        query.extend([
            ("state", pkce.state.as_str()),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
            ("redirect_uri", self.config.redirect_uri.as_str()),
        ]);
        http::url(self.config.connect_base(), &["oauth2", "authorize"], &query)
    }

    /// Whether a sign-in is kept; a store that can't tell says so.
    pub fn signed_in(&self) -> Result<bool, StoreError> {
        Ok(self.store.load()?.is_some())
    }

    /// Square's redirect, back in the app: the code for this attempt's
    /// `pkce`. Another attempt's `state`, or no `code`, is refused before
    /// Square is asked; an `error` ends the attempt. The tokens are kept
    /// before they're answered.
    pub async fn finish(
        &self,
        pkce: &Pkce,
        state: Option<&str>,
        code: Option<&str>,
        error: Option<&str>,
    ) -> Result<PosTokens, PosError> {
        if let Some(error) = error {
            return Err(PosError::answered(
                PointOfSale::Square,
                0,
                format!("Square sign-in was cancelled or refused ({error})."),
            ));
        }
        let code = match code {
            Some(code) if !code.is_empty() && state == Some(pkce.state.as_str()) => code,
            _ => {
                return Err(PosError::answered(
                    PointOfSale::Square,
                    0,
                    "That sign-in wasn't the one this app started. Try again.",
                ));
            }
        };
        let tokens = self
            .token(json!({
                "client_id": self.config.application_id,
                "grant_type": "authorization_code",
                "code": code,
                "redirect_uri": self.config.redirect_uri,
                "code_verifier": pkce.verifier,
            }))
            .await?;
        let _turn = self.turn.lock().await;
        let _held = hold(&self.lock).await?;
        self.store.save(Some(tokens.clone()));
        Ok(tokens)
    }

    /// An access token good for at least a day, refreshed first when it
    /// isn't. A refresh Square refuses (any `4xx`) forgets the sign-in and
    /// says to sign in again.
    pub async fn access_token(&self) -> Result<String, PosError> {
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
        let Some(refresh) = tokens.refresh_token else {
            return Err(ended(401));
        };
        let renewed = match self
            .token(json!({
                "client_id": self.config.application_id,
                "grant_type": "refresh_token",
                "refresh_token": refresh,
            }))
            .await
        {
            Ok(renewed) => renewed,
            Err(e) => match e.status() {
                Some(status @ 400..=499) => {
                    self.store.save(None);
                    return Err(ended(status));
                }
                _ => return Err(e),
            },
        };
        // The old refresh token is spent: keep the new one first.
        self.store.save(Some(renewed.clone()));
        Ok(renewed.access_token)
    }

    /// Forgets the sign-in.
    pub fn sign_out(&self) {
        self.store.save(None);
    }

    fn signed_in_tokens(&self) -> Result<PosTokens, PosError> {
        self.store
            .load()?
            .ok_or_else(|| PosError::sign_in(PointOfSale::Square, 401, "Sign in to Square."))
    }

    /// Whether `tokens`' access token has more than a day left.
    fn lasts(&self, tokens: &PosTokens) -> bool {
        let due =
            crate::time::from_seconds(self.clock.now().saturating_add(REFRESH_BEFORE_SECONDS));
        tokens.expires_at.timestamp() > due
    }

    async fn token(&self, body: serde_json::Value) -> Result<PosTokens, PosError> {
        let request = self
            .http
            .post(format!("{}/oauth2/token", self.config.connect_base()))
            .header("Square-Version", SquareConfig::API_VERSION)
            .json(&body);
        let answer = http::send(request).await?;
        if !answer.ok() {
            return Err(answered(answer.status, &answer.body));
        }
        http::read(PointOfSale::Square, &answer.body)
    }
}

/// The sign-in has ended.
fn ended(status: u16) -> PosError {
    PosError::sign_in(
        PointOfSale::Square,
        status,
        "Square's sign-in has ended. Sign in again.",
    )
}
