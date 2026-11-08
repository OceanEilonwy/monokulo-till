//! Monokular's points of sale (docs/design.md §1): signing in to Square and
//! Clover with PKCE, finding the Monero sales they take, refunding them, and
//! turning their orders into ORF receipts.
//!
//! Ported from the Android app's `sales` module (docs/ios-plan.md §3,
//! phase 2). Nothing here knows about a platform or about UniFFI.
