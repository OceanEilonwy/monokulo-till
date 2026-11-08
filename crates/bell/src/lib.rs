//! Wake-ups from a Monokular Bell (docs/design.md §3): its address, its API,
//! registering a till with it and keeping that registration in line with the
//! till's choice, and which pushes count as dings.
//!
//! Ported from the Android app's `push` module (docs/ios-plan.md §3,
//! phase 4). Nothing here knows about a platform or about UniFFI.
