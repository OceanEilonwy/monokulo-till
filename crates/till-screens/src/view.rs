//! The parts the screens' records are made of.

use monokulo_client::PointOfSale;

/// What a screen is drawn on, where its words differ: iOS has no
/// UnifiedPush (its wake-ups arrive as alert pushes through the relay), no
/// bubble and no Tor, and the iPhone has the Dynamic Island.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum Device {
    Android,
    Iphone,
    Ipad,
}

impl Device {
    pub fn is_ios(self) -> bool {
        matches!(self, Device::Iphone | Device::Ipad)
    }
}

/// The colour role of a value or a row that's a button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum Tone {
    /// The ink, or the secondary ink for a fact's value.
    Plain,
    /// Quieter than plain: a status that isn't good news yet.
    Muted,
    /// The paid green: connected.
    Paid,
    /// Something to do before it works.
    Warn,
    /// A row that does something.
    Link,
    /// A row that undoes something: Disconnect.
    Danger,
}

/// A row with a name and its value: "Payments go to · Shop wallet".
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct Fact {
    pub label: String,
    pub value: String,
    pub tone: Tone,
    /// Set in the monospaced face: a wallet address.
    pub mono: bool,
}

impl Fact {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Fact {
        Fact {
            label: label.into(),
            value: value.into(),
            tone: Tone::Plain,
            mono: false,
        }
    }

    pub fn toned(self, tone: Tone) -> Fact {
        Fact { tone, ..self }
    }

    pub fn mono(self) -> Fact {
        Fact { mono: true, ..self }
    }
}

/// A point of sale's mark beside its name, each brand's own art.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum PosMark {
    Square,
    Clover,
}

/// One row of a radio group: a point of sale, a way wake-ups come.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct Choice {
    pub label: String,
    pub detail: Option<String>,
    pub selected: bool,
    pub enabled: bool,
}

/// A point of sale as a choice row, led by its mark on a tile.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct PosChoice {
    pub mark: PosMark,
    /// Whether the mark is drawn in the theme's ink (Square's), or keeps its
    /// own colour (Clover's green).
    pub mark_in_ink: bool,
    pub choice: Choice,
}

/// A button: its words, and whether it can be pressed now.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct Press {
    pub label: String,
    pub enabled: bool,
}

impl Press {
    pub fn new(label: impl Into<String>) -> Press {
        Press {
            label: label.into(),
            enabled: true,
        }
    }

    pub fn enabled(self, enabled: bool) -> Press {
        Press { enabled, ..self }
    }
}

/// `pos`'s mark.
pub fn mark(pos: PointOfSale) -> PosMark {
    match pos {
        PointOfSale::Square => PosMark::Square,
        PointOfSale::Clover => PosMark::Clover,
    }
}

/// "On" or "Off".
pub(crate) fn on_off(on: bool) -> &'static str {
    if on { "On" } else { "Off" }
}

/// The first letter in upper case: "mainnet" is "Mainnet".
pub(crate) fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
