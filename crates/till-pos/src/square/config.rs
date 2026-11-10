//! Which Square, and as which application (Android: `SquareConfig.kt`).

use crate::SquareEnvironment;

/// Which Square, and as which application (docs/design.md §1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SquareConfig {
    /// The Square application's id (public: no secret is ever on the
    /// device).
    pub application_id: String,
    pub sandbox: bool,
    /// Where Square sends the sign-in back: an App Link that opens the app.
    pub redirect_uri: String,
    /// The name of the Other tender the cashier taps.
    pub tender_name: String,
    /// Square's address, for tests; otherwise production's or the
    /// sandbox's.
    pub base_url: Option<String>,
}

impl SquareConfig {
    /// The API version every request asks for (decision 20).
    pub const API_VERSION: &'static str = "2026-09-16";

    /// What the till asks Square to let it do.
    pub const SCOPES: [&'static str; 4] = [
        "PAYMENTS_READ",
        "ORDERS_READ",
        "MERCHANT_PROFILE_READ",
        "PAYMENTS_WRITE",
    ];

    /// The application at production's Square or the sandbox's, with the
    /// tender named "Monero".
    pub fn new(
        application_id: impl Into<String>,
        sandbox: bool,
        redirect_uri: impl Into<String>,
    ) -> SquareConfig {
        SquareConfig {
            application_id: application_id.into(),
            sandbox,
            redirect_uri: redirect_uri.into(),
            tender_name: "Monero".into(),
            base_url: None,
        }
    }

    /// Where Square's sign-in and API are.
    pub fn connect_base(&self) -> &str {
        match &self.base_url {
            Some(base) => base,
            None if self.sandbox => "https://connect.squareupsandbox.com",
            None => "https://connect.squareup.com",
        }
    }

    /// The environment [`application_id`](Self::application_id) is in
    /// (decision 147).
    pub fn environment(&self) -> SquareEnvironment {
        SquareEnvironment::of_application(&self.application_id)
    }
}
