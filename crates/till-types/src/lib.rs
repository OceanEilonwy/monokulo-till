//! Small types shared by the till's Rust core, so a value's meaning is in
//! its type rather than in a `String`: [`Secret`] for credentials that must
//! never reach a log, [`text_id!`] for opaque ids that mustn't be
//! passed one for another, [`wire_enum!`] for a closed set of words
//! another service sends, with room for one it adds later,
//! [`CurrencyCode`] and [`Money`], with the one ISO 4217 table, and
//! [`UnixSeconds`] and [`UnixMillis`] for the two units the core keeps
//! times in.

pub mod money;
mod secret;
mod time;

pub use money::{CurrencyCode, Money, NotACurrencyCode, Price, PriceUsed};
pub use secret::Secret;
pub use time::{UnixMillis, UnixSeconds};

/// A newtype over `String` for an opaque id: `Clone, Debug, Eq, Ord, Hash`,
/// serialized as the bare string (`#[serde(transparent)]`), with `new`,
/// `as_str`, `Display`, `From<&str>`/`From<String>`, and read as its text
/// (`Deref<Target = str>`, `AsRef<str>`, `Borrow<str>`, comparison with
/// text).
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

        impl ::std::convert::From<&String> for $name {
            fn from(id: &String) -> $name {
                $name(id.clone())
            }
        }

        impl ::std::convert::From<&$name> for $name {
            fn from(id: &$name) -> $name {
                id.clone()
            }
        }

        /// Read as its text; made only through `new` and `From`.
        impl ::std::ops::Deref for $name {
            type Target = str;

            fn deref(&self) -> &str {
                &self.0
            }
        }

        impl ::std::convert::AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl ::std::borrow::Borrow<str> for $name {
            fn borrow(&self) -> &str {
                &self.0
            }
        }

        impl ::std::cmp::PartialEq<str> for $name {
            fn eq(&self, other: &str) -> bool {
                self.0 == other
            }
        }

        impl ::std::cmp::PartialEq<&str> for $name {
            fn eq(&self, other: &&str) -> bool {
                self.0 == *other
            }
        }

        impl ::std::cmp::PartialEq<String> for $name {
            fn eq(&self, other: &String) -> bool {
                self.0 == *other
            }
        }

        impl ::std::cmp::PartialEq<&$name> for $name {
            fn eq(&self, other: &&$name) -> bool {
                self == *other
            }
        }

        impl ::std::cmp::PartialEq<$name> for String {
            fn eq(&self, other: &$name) -> bool {
                *self == other.0
            }
        }

        impl ::std::cmp::PartialEq<&$name> for String {
            fn eq(&self, other: &&$name) -> bool {
                *self == other.0
            }
        }
    };
}

/// An enum for a set of words a service writes (a payment's `COMPLETED`),
/// each variant with its exact text, and `Other(String)` for any text this
/// build doesn't know, kept as it came: serialized as the bare text either
/// way, so a value read and written back is unchanged. `Clone, Debug, Eq,
/// Ord, Hash`, with `as_str`, `is_known`, `Display` and `From<&str>`/
/// `From<String>`. Reading never fails.
///
/// ```
/// till_types::wire_enum!(
///     /// A light's state.
///     Light { On = "ON", Off = "OFF" }
/// );
/// assert_eq!(Light::from("ON"), Light::On);
/// assert_eq!(Light::from("DIM"), Light::Other("DIM".into()));
/// assert_eq!(Light::from("DIM").as_str(), "DIM");
/// ```
#[macro_export]
macro_rules! wire_enum {
    ($(#[$doc:meta])* $name:ident { $($(#[$vdoc:meta])* $variant:ident = $text:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum $name {
            $($(#[$vdoc])* $variant,)+
            /// Text this build doesn't know, as it came.
            Other(String),
        }

        impl $name {
            /// The text, as the service writes it.
            pub fn as_str(&self) -> &str {
                match self {
                    $($name::$variant => $text,)+
                    $name::Other(text) => text,
                }
            }

            /// Whether this build knows it: not [`Other`](Self::Other).
            pub fn is_known(&self) -> bool {
                !matches!(self, $name::Other(_))
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl ::std::convert::From<&str> for $name {
            fn from(text: &str) -> $name {
                match text {
                    $($text => $name::$variant,)+
                    other => $name::Other(other.to_owned()),
                }
            }
        }

        impl ::std::convert::From<String> for $name {
            fn from(text: String) -> $name {
                $name::from(text.as_str())
            }
        }

        impl ::serde::Serialize for $name {
            fn serialize<S: ::serde::Serializer>(&self, s: S) -> ::std::result::Result<S::Ok, S::Error> {
                s.serialize_str(self.as_str())
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(d: D) -> ::std::result::Result<$name, D::Error> {
                let text = <::std::string::String as ::serde::Deserialize>::deserialize(d)?;
                Ok($name::from(text))
            }
        }
    };
}

#[cfg(test)]
mod wire_enum_tests {
    wire_enum!(Light { On = "ON", Off = "OFF" });

    #[test]
    fn known_and_unknown_text_round_trip() {
        for text in ["\"ON\"", "\"OFF\"", "\"DIM\""] {
            let light: Light = serde_json::from_str(text).unwrap();
            assert_eq!(serde_json::to_string(&light).unwrap(), text);
        }
        assert_eq!(Light::from("OFF"), Light::Off);
        assert!(Light::On.is_known());
        assert!(!Light::from("on").is_known(), "the text is exact");
    }
}
