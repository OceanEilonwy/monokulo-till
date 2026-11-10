//! Which Clover, and as which application (Android: `CloverConfig.kt`).

pub use crate::environment::CloverEnvironment;

/// Which Clover, and as which application (docs/design.md §1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloverConfig {
    /// The Clover app's id (public: no secret is ever on the device).
    pub application_id: String,
    pub environment: CloverEnvironment,
    /// Where Clover sends the sign-in back: an App Link under the Clover
    /// app's Site URL.
    pub redirect_uri: String,
    /// The label of the tender the cashier taps in Clover Register.
    pub tender_label: String,
    /// Clover requires a `User-Agent` on every request.
    pub user_agent: String,
    /// A stand-in Clover's address (fake-clover, tests): both its sign-in
    /// page and its API.
    pub base_url: Option<String>,
}

impl CloverConfig {
    /// The application in `environment`, with the tender labelled "Monero"
    /// and the user agent "Till".
    pub fn new(
        application_id: impl Into<String>,
        environment: CloverEnvironment,
        redirect_uri: impl Into<String>,
    ) -> CloverConfig {
        CloverConfig {
            application_id: application_id.into(),
            environment,
            redirect_uri: redirect_uri.into(),
            tender_label: "Monero".into(),
            user_agent: "Till".into(),
            base_url: None,
        }
    }

    /// Where the merchant signs in.
    pub fn web_base(&self) -> &str {
        self.base_url
            .as_deref()
            .unwrap_or(self.environment.web_base())
    }

    /// Where tokens and `/v3` are.
    pub fn api_base(&self) -> &str {
        self.base_url
            .as_deref()
            .unwrap_or(self.environment.api_base())
    }
}
