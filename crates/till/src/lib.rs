//! The till (docs/design.md §1, §3): the shift's sales and where each stands,
//! making Monokulo orders for the sales a point of sale takes, and the
//! wake-up loop that decides when to look.
//!
//! Ported from the Android app's `Till`, `Standing`, `Shift` and
//! `TillRunner` (docs/ios-plan.md §3, phase 3). Nothing here knows about a
//! platform or about UniFFI.
