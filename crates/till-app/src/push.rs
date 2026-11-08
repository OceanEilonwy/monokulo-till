//! The platform's push side as till-bell takes it (decision 19): its
//! `watch` and `broadcast` receivers can't cross UniFFI, so the platform
//! calls [`PushPlatform`] one way and answers with actions, which the
//! engine hands to a [`PushFeed`].

use std::sync::Arc;

use till_bell::{
    DeviceError, Distributor, Distributors as BellDistributors, PushFailure as BellFailure,
    PushFeed, PushState, PushSubscriptions,
};
use tokio::sync::{broadcast, watch};

use crate::{Distributors, PushFailure, PushPlatform};

/// [`PushSubscriptions`] and till-bell's `Distributors` over the platform.
pub(crate) struct PlatformPush {
    pub feed: PushFeed,
    platform: Arc<dyn PushPlatform>,
    changes: broadcast::Sender<()>,
}

impl PlatformPush {
    pub fn new(platform: Arc<dyn PushPlatform>) -> PlatformPush {
        PlatformPush {
            feed: PushFeed::new(),
            platform,
            changes: broadcast::Sender::new(1),
        }
    }

    /// An app came or went.
    pub fn distributors_changed(&self) {
        let _ = self.changes.send(());
    }

    pub fn failed(&self, failure: PushFailure) {
        self.feed.set(PushState::Failed(match failure {
            PushFailure::NoDistributor => BellFailure::NoDistributor,
            PushFailure::Network => BellFailure::Network,
            PushFailure::ActionRequired => BellFailure::ActionRequired,
            PushFailure::NoKeys => BellFailure::NoKeys,
            PushFailure::Lost => BellFailure::Lost,
            PushFailure::Other => BellFailure::Other,
        }));
    }
}

impl PushSubscriptions for PlatformPush {
    fn state(&self) -> watch::Receiver<PushState> {
        self.feed.state()
    }

    fn dings(&self) -> broadcast::Receiver<()> {
        self.feed.dings()
    }

    fn subscribe(&self, vapid_public_key: &str) {
        self.feed.set(PushState::Subscribing);
        self.platform.subscribe(vapid_public_key.to_owned());
    }

    fn unsubscribe(&self) {
        self.feed.set(PushState::None);
        self.platform.unsubscribe();
    }
}

impl BellDistributors for PlatformPush {
    fn resolve(&self) -> Result<Distributor, DeviceError> {
        match self.platform.distributors() {
            Ok(Distributors::Found { name }) => Ok(Distributor::Found(name)),
            Ok(Distributors::None) => Ok(Distributor::None),
            Ok(Distributors::Choose { names }) => Ok(Distributor::Choose(names)),
            Err(e) => Err(DeviceError(e.to_string())),
        }
    }

    fn save(&self, package_name: &str) {
        self.platform.save_distributor(package_name.to_owned());
    }

    fn changes(&self) -> broadcast::Receiver<()> {
        self.changes.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakePush;

    #[test]
    fn subscribing_asks_the_platform_and_its_answers_feed_the_state() {
        let platform = Arc::new(FakePush::default());
        let push = PlatformPush::new(platform.clone());
        let state = push.state();
        push.subscribe("BKey");
        assert_eq!(*state.borrow(), PushState::Subscribing);
        push.feed
            .endpoint("https://push.example/1", Some("p"), Some("a"));
        assert!(matches!(*state.borrow(), PushState::Subscribed(_)));
        push.failed(PushFailure::Network);
        assert_eq!(*state.borrow(), PushState::Failed(BellFailure::Network));
        push.unsubscribe();
        assert_eq!(*state.borrow(), PushState::None);
        assert_eq!(
            *platform.asked.lock().unwrap(),
            ["subscribe BKey", "unsubscribe"]
        );
        assert_eq!(push.resolve().unwrap(), Distributor::Found("apns".into()));
    }
}
