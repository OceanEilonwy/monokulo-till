//! The device's own wallet as Till's payment backend (docs/view-key-mode.md
//! §5, core.md decisions 229 to 233): Fob's wallet, `fob-wallet`, as a
//! library in Manual Sync, view-only, watching one account per till.
//!
//! - [`WalletBackend`]: `PaymentBackend` over the wallet. Payments are
//!   Fob's keyed requests, on a small pool of addresses in the till's
//!   account with an amount tag each; where each stands is mokulo's status
//!   ([`status`]) over the payments toward it. It looks at the chain only
//!   while its event stream is polled.
//! - [`prices`]: the sale's currency in XMR, from fob-price, never from a
//!   price older than ten minutes.
//! - [`keys`]: the ways in: typed, a wallet code, or a seed phrase that is
//!   wiped as soon as the view key is taken from it.
//! - [`connect`]: Connect to Fob, Till's side.
//! - [`setup`]: the wallet file made at the end of setup, and the checks
//!   of a node or of the shop's light wallet server.

pub mod backend;
pub mod connect;
pub mod keys;
pub mod prices;
pub mod setup;
pub mod status;

pub use backend::{WALLET_FILE, WalletBackend, wallet_path};
pub use connect::{AnswerProblem, FobRequest, Granted, Pattern};
pub use keys::{Network, ViewKeys};
pub use setup::{
    CheckState, LightCheck, Made, NodeCheck, WalletSetup, check_light_server, make_wallet,
    remove_wallet,
};
