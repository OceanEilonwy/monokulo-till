//! Which Clover, and as which application (Android: `CloverConfig.kt`).

/// Which of Clover's platforms a merchant is on: each has its own sign-in
/// page and API host, and its own developer apps (decision 127 in
/// docs/decisions/android.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CloverEnvironment {
    Sandbox,
    NorthAmerica,
    Europe,
    LatinAmerica,
}

impl CloverEnvironment {
    /// Every environment, in the order the Bell lists them.
    pub const ALL: [CloverEnvironment; 4] = [
        CloverEnvironment::Sandbox,
        CloverEnvironment::NorthAmerica,
        CloverEnvironment::Europe,
        CloverEnvironment::LatinAmerica,
    ];

    /// The name builds and the Bell's `/v1/info` use for it.
    pub fn wire(self) -> &'static str {
        match self {
            CloverEnvironment::Sandbox => "sandbox",
            CloverEnvironment::NorthAmerica => "north_america",
            CloverEnvironment::Europe => "europe",
            CloverEnvironment::LatinAmerica => "latin_america",
        }
    }

    /// Where the merchant signs in.
    pub fn web_base(self) -> &'static str {
        match self {
            CloverEnvironment::Sandbox => "https://sandbox.dev.clover.com",
            CloverEnvironment::NorthAmerica => "https://www.clover.com",
            CloverEnvironment::Europe => "https://www.eu.clover.com",
            CloverEnvironment::LatinAmerica => "https://www.la.clover.com",
        }
    }

    /// Tokens and `/v3`.
    pub fn api_base(self) -> &'static str {
        match self {
            CloverEnvironment::Sandbox => "https://apisandbox.dev.clover.com",
            CloverEnvironment::NorthAmerica => "https://api.clover.com",
            CloverEnvironment::Europe => "https://api.eu.clover.com",
            CloverEnvironment::LatinAmerica => "https://api.la.clover.com",
        }
    }

    /// The environment `wire` names, or `None` for one this build doesn't
    /// know.
    pub fn from_wire(wire: &str) -> Option<CloverEnvironment> {
        CloverEnvironment::ALL
            .into_iter()
            .find(|e| e.wire() == wire)
    }
}

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
