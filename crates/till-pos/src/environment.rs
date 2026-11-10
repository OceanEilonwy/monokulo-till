//! Each point of sale's environments, as builds and a Bell's `/v1/info`
//! spell them (decisions 147 and 305): ungated, so a crate that only names
//! them needn't build either point of sale's client.

use crate::PointOfSale;

/// Which Square an application is in: production's or the sandbox's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SquareEnvironment {
    Sandbox,
    Production,
}

impl SquareEnvironment {
    pub const ALL: [SquareEnvironment; 2] =
        [SquareEnvironment::Production, SquareEnvironment::Sandbox];

    /// The name a Bell's `/v1/info` uses for it.
    pub fn wire(self) -> &'static str {
        match self {
            SquareEnvironment::Sandbox => "sandbox",
            SquareEnvironment::Production => "production",
        }
    }

    /// The environment `wire` names, or `None` for one this build doesn't
    /// know.
    pub fn from_wire(wire: &str) -> Option<SquareEnvironment> {
        SquareEnvironment::ALL
            .into_iter()
            .find(|e| e.wire() == wire)
    }

    /// The environment `application_id` is in: Square's sandbox
    /// applications' ids start `sandbox-`; every other id is production's.
    pub fn of_application(application_id: &str) -> SquareEnvironment {
        if application_id.starts_with("sandbox-") {
            SquareEnvironment::Sandbox
        } else {
            SquareEnvironment::Production
        }
    }
}

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

/// The environment a till signed in through, at either point of sale: what
/// registering with a Bell names (decision 305).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PosEnvironment {
    Square(SquareEnvironment),
    Clover(CloverEnvironment),
}

impl PosEnvironment {
    pub fn point_of_sale(self) -> PointOfSale {
        match self {
            PosEnvironment::Square(_) => PointOfSale::Square,
            PosEnvironment::Clover(_) => PointOfSale::Clover,
        }
    }

    /// The environment's name, as a Bell spells it under its point of sale.
    pub fn wire(self) -> &'static str {
        match self {
            PosEnvironment::Square(environment) => environment.wire(),
            PosEnvironment::Clover(environment) => environment.wire(),
        }
    }

    /// `point_of_sale`'s environment `wire` names; `None` for one this build
    /// doesn't know.
    pub fn from_wire(point_of_sale: PointOfSale, wire: &str) -> Option<PosEnvironment> {
        match point_of_sale {
            PointOfSale::Square => SquareEnvironment::from_wire(wire).map(PosEnvironment::Square),
            PointOfSale::Clover => CloverEnvironment::from_wire(wire).map(PosEnvironment::Clover),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_environment_is_named_as_a_bell_names_it() {
        let every: Vec<PosEnvironment> = SquareEnvironment::ALL
            .map(PosEnvironment::Square)
            .into_iter()
            .chain(CloverEnvironment::ALL.map(PosEnvironment::Clover))
            .collect();
        for environment in every {
            assert_eq!(
                PosEnvironment::from_wire(environment.point_of_sale(), environment.wire()),
                Some(environment)
            );
        }
        assert_eq!(
            SquareEnvironment::ALL.map(SquareEnvironment::wire),
            ["production", "sandbox"]
        );
        assert_eq!(
            PosEnvironment::from_wire(PointOfSale::Square, "europe"),
            None,
            "Clover's, not Square's"
        );
        assert_eq!(
            PosEnvironment::from_wire(PointOfSale::Clover, "production"),
            None
        );
    }
}
