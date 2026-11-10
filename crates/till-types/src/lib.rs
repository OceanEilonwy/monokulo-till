//! Small types shared by the till's Rust core, so a value's meaning is in
//! its type rather than in a `String`: [`Secret`] for credentials that must
//! never reach a log, and [`text_id!`] for opaque ids that mustn't be
//! passed one for another.

mod secret;

pub use secret::Secret;

/// A newtype over `String` for an opaque id: `Clone, Debug, Eq, Ord, Hash`,
/// serialized as the bare string (`#[serde(transparent)]`), with `new`,
/// `as_str`, `Display` and `From<&str>`/`From<String>`.
///
/// ```
/// till_types::text_id!(
///     /// A widget's id.
///     WidgetId
/// );
/// let id = WidgetId::new("w1");
/// assert_eq!(id.as_str(), "w1");
/// ```
#[macro_export]
macro_rules! text_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(
            Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
            ::serde::Serialize, ::serde::Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(id: impl Into<String>) -> $name {
                $name(id.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl ::std::convert::From<&str> for $name {
            fn from(id: &str) -> $name {
                $name(id.to_owned())
            }
        }

        impl ::std::convert::From<String> for $name {
            fn from(id: String) -> $name {
                $name(id)
            }
        }
    };
}
