//! Changing the point of sale (Android: `PosSwitch.kt`).

use async_trait::async_trait;
use monokulo_client::PointOfSale;

/// What changing the point of sale touches, in the order [`PosSwitch`]
/// does it: the engine and the stores behind it, a recording fake in the
/// tests.
#[async_trait]
pub trait PosSwitchSide: Send + Sync {
    /// The Bell's registration taken back, awaited, while the old sign-in
    /// is still here (acceptance T5.5).
    async fn take_bell_registration_back(&self);

    /// Signed out of `pos`: its tokens, the application they came through,
    /// its location or merchant and its remembered tender id forgotten.
    fn sign_out(&self, pos: PointOfSale);

    /// Where `pos`'s search for sales had got to forgotten, so a sign-in to
    /// it later starts from then.
    fn forget_cursor(&self, pos: PointOfSale);

    /// `pos` saved as the till's point of sale.
    fn choose(&self, pos: PointOfSale);
}

/// Choosing a point of sale (acceptance D6, decision 138 in
/// docs/decisions/android.md), from onboarding's rows or Settings' "Change
/// the point of sale", and signing out of one.
pub struct PosSwitch;

impl PosSwitch {
    /// Chooses `to` in place of `from`, answering whether anything changed:
    /// the caller then shows the new point of sale's sign-in.
    ///
    /// Choosing the one chosen changes nothing. Choosing another takes the
    /// Bell's registration back first when still signed in to the first,
    /// then always signs out of the old one, so a sign-in that had already
    /// ended leaves no location, merchant, cursor or tender id behind, then
    /// saves the new. The Wake-ups choice and the shift's saved sales are
    /// never touched, so the Bell registers again for the new point of sale
    /// once it is signed in, and the old sales' pages still open, named by
    /// the point of sale that took them ([`till_pos::FoundSale::point_of_sale`]).
    pub async fn choose(
        side: &dyn PosSwitchSide,
        from: Option<PointOfSale>,
        signed_in: bool,
        to: PointOfSale,
    ) -> bool {
        if from == Some(to) {
            return false;
        }
        if let Some(from) = from {
            if signed_in {
                side.take_bell_registration_back().await;
            }
            side.sign_out(from);
            side.forget_cursor(from);
        }
        side.choose(to);
        true
    }

    /// Settings' "Sign out of …": the Bell's registration taken back,
    /// awaited, then the sign-in forgotten. The cursor stays, as Square's
    /// sign-out always kept it: signing in to the same point of sale again
    /// carries on from where the till had got to rather than reading the
    /// merchant's earlier payments again (decision 138).
    pub async fn sign_out(side: &dyn PosSwitchSide, pos: PointOfSale) {
        side.take_bell_registration_back().await;
        side.sign_out(pos);
    }
}

#[cfg(test)]
mod tests {
    //! Ported from the Android app's `PosSwitchTest`: changing the point of
    //! sale (acceptance T1.4, D6; decision 138), in order, and only what it
    //! should touch.

    use std::sync::Mutex;

    use super::*;

    /// The app's side as a log, with the two things a switch must never
    /// touch beside it.
    struct Recording {
        log: Mutex<Vec<String>>,
        wake_up_choice: &'static str,
        saved_sales: Vec<&'static str>,
    }

    impl Recording {
        fn new() -> Recording {
            Recording {
                log: Mutex::new(Vec::new()),
                wake_up_choice: "Monokulo Bell",
                saved_sales: vec!["bP9m", "k4Rt"],
            }
        }

        fn log(&self) -> Vec<String> {
            self.log.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl PosSwitchSide for Recording {
        async fn take_bell_registration_back(&self) {
            self.log
                .lock()
                .unwrap()
                .push("Bell: DELETE, awaited".into());
        }
        fn sign_out(&self, pos: PointOfSale) {
            self.log
                .lock()
                .unwrap()
                .push(format!("sign out of {}", pos.wire()));
        }
        fn forget_cursor(&self, pos: PointOfSale) {
            self.log
                .lock()
                .unwrap()
                .push(format!("forget {} cursor", pos.wire()));
        }
        fn choose(&self, pos: PointOfSale) {
            self.log
                .lock()
                .unwrap()
                .push(format!("choose {}", pos.wire()));
        }
    }

    #[tokio::test]
    async fn switching_while_signed_in_takes_the_bells_registration_back_then_signs_out_then_chooses()
     {
        let side = Recording::new();
        assert!(
            PosSwitch::choose(&side, Some(PointOfSale::Square), true, PointOfSale::Clover).await
        );
        assert_eq!(
            side.log(),
            [
                "Bell: DELETE, awaited",
                "sign out of square",
                "forget square cursor",
                "choose clover"
            ]
        );
        assert_eq!(
            side.wake_up_choice, "Monokulo Bell",
            "the Wake-ups choice stays"
        );
        assert_eq!(side.saved_sales, ["bP9m", "k4Rt"], "the shift's sales stay");
    }

    #[tokio::test]
    async fn and_back_the_other_way() {
        let side = Recording::new();
        assert!(
            PosSwitch::choose(&side, Some(PointOfSale::Clover), true, PointOfSale::Square).await
        );
        assert_eq!(
            side.log(),
            [
                "Bell: DELETE, awaited",
                "sign out of clover",
                "forget clover cursor",
                "choose square"
            ]
        );
    }

    #[tokio::test]
    async fn choosing_the_one_chosen_changes_nothing() {
        for pos in PointOfSale::ALL {
            let side = Recording::new();
            assert!(!PosSwitch::choose(&side, Some(pos), true, pos).await);
            assert!(!PosSwitch::choose(&side, Some(pos), false, pos).await);
            assert!(side.log().is_empty(), "{}", pos.wire());
        }
    }

    #[tokio::test]
    async fn a_first_choice_only_chooses_and_one_after_the_sign_in_ended_still_signs_out() {
        let first = Recording::new();
        assert!(PosSwitch::choose(&first, None, false, PointOfSale::Clover).await);
        assert_eq!(first.log(), ["choose clover"]);
        let not_signed_in = Recording::new();
        assert!(
            PosSwitch::choose(
                &not_signed_in,
                Some(PointOfSale::Square),
                false,
                PointOfSale::Clover
            )
            .await
        );
        assert_eq!(
            not_signed_in.log(),
            [
                "sign out of square",
                "forget square cursor",
                "choose clover"
            ],
            "no registration to take back, but the old location, cursor and tender id go"
        );
    }

    #[tokio::test]
    async fn signing_out_takes_the_bells_registration_back_first_and_keeps_the_cursor_so_signing_in_again_reads_on_from_there()
     {
        for pos in PointOfSale::ALL {
            let side = Recording::new();
            PosSwitch::sign_out(&side, pos).await;
            assert_eq!(
                side.log(),
                [
                    "Bell: DELETE, awaited".to_owned(),
                    format!("sign out of {}", pos.wire())
                ],
                "{}",
                pos.wire()
            );
        }
    }
}
