use crate::nintendo::{
    self, Friend,
    installation::{self, Cancellation, Stage},
};
use std::{
    sync::{
        Arc,
        mpsc::{self, Receiver, TryRecvError},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Default)]
pub enum ComponentStatus {
    #[default]
    Unknown,
    Missing,
    Installing(Stage),
    Ready,
    Failed(String),
    Cancelled,
}

enum Update {
    Stage(Stage),
    Finished(Result<(), String>),
}

#[derive(Default)]
pub(super) struct Nintendo {
    pub friends: Vec<Friend>,
    pub error: Option<String>,
    pub game: Option<String>,
    pub started_at: i64,
    pub status: ComponentStatus,
    pending: Option<Receiver<Result<Vec<Friend>, String>>>,
    last_attempt: Option<Instant>,
    last_success: Option<Instant>,
    install: Option<Receiver<Update>>,
    cancellation: Arc<Cancellation>,
}

impl Nintendo {
    fn set_game(&mut self, game: Option<String>) {
        if self.game != game {
            self.game = game;
            self.started_at = super::unix_now();
        }
    }
    pub fn select(&mut self, id: Option<&str>) {
        let fresh = self
            .last_success
            .is_some_and(|t| t.elapsed() < Duration::from_secs(90));
        self.set_game(
            id.filter(|_| fresh)
                .and_then(|id| nintendo::selected_game(&self.friends, id))
                .map(str::to_owned),
        );
    }
    fn accept(&mut self, result: Result<Vec<Friend>, String>) {
        match result {
            Ok(friends) => {
                self.friends = friends;
                self.last_success = Some(Instant::now());
                self.error = None;
            }
            Err(error) => {
                self.friends.clear();
                self.last_success = None;
                self.error = Some(error);
                self.set_game(None);
            }
        }
    }
    pub fn refresh(&mut self) {
        self.last_attempt = None;
    }
    pub fn pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn poll(&mut self, enabled: bool, friend_id: &mut String, network: bool) -> bool {
        let mut dirty = false;
        loop {
            match self.install.as_ref().map(Receiver::try_recv) {
                Some(Ok(Update::Stage(stage))) => self.status = ComponentStatus::Installing(stage),
                Some(Ok(Update::Finished(result))) => {
                    self.status = match result {
                        Ok(()) => {
                            self.refresh();
                            ComponentStatus::Ready
                        }
                        Err(e) => ComponentStatus::Failed(e),
                    };
                    self.install = None;
                }
                Some(Err(TryRecvError::Disconnected)) => {
                    self.status =
                        ComponentStatus::Failed("Установка прервана. Можно повторить.".into());
                    self.install = None;
                }
                _ => break,
            }
        }
        if !enabled {
            // Dropping the receiver rejects a response from a previous enabled generation.
            self.pending = None;
            self.last_attempt = None;
            self.friends.clear();
            self.last_success = None;
            self.set_game(None);
            return false;
        }
        match self.pending.as_ref().map(Receiver::try_recv) {
            Some(Ok(result)) => {
                self.pending = None;
                self.accept(result);
            }
            Some(Err(TryRecvError::Disconnected)) => {
                self.pending = None;
                self.accept(Err("Проверка Nintendo прервана.".into()));
            }
            _ => {}
        }
        if friend_id.is_empty() && self.friends.len() == 1 {
            *friend_id = self.friends[0].id.clone();
            dirty = true;
        }
        self.select(Some(friend_id));
        if network
            && self.pending.is_none()
            && self
                .last_attempt
                .is_none_or(|t| t.elapsed() >= Duration::from_secs(60))
        {
            let (tx, rx) = mpsc::channel();
            self.pending = Some(rx);
            self.last_attempt = Some(Instant::now());
            std::thread::spawn(move || {
                let _ = tx.send(nintendo::fetch());
            });
        }
        dirty
    }
    pub fn check_components(&mut self) {
        if self.install.is_none() {
            self.status = if nintendo::helper_directory()
                .ok()
                .and_then(|p| installation::installed(&p))
                .is_some()
            {
                ComponentStatus::Ready
            } else {
                ComponentStatus::Missing
            };
        }
    }
    pub fn install(&mut self) {
        if self.install.is_some() {
            return;
        }
        let base = match nintendo::helper_directory() {
            Ok(base) => base,
            Err(e) => {
                self.status = ComponentStatus::Failed(e);
                return;
            }
        };
        let (tx, rx) = mpsc::channel();
        self.install = Some(rx);
        self.cancellation = Arc::new(Cancellation::default());
        let cancel = self.cancellation.clone();
        self.status = ComponentStatus::Installing(Stage::Downloading);
        std::thread::spawn(move || {
            let result = installation::install(
                &base,
                |stage| {
                    let _ = tx.send(Update::Stage(stage));
                },
                &cancel,
            )
            .map(|_| ());
            let _ = tx.send(Update::Finished(result));
        });
    }
    pub fn cancel(&mut self) {
        self.cancellation.cancel();
        self.install = None;
        self.status = ComponentStatus::Cancelled;
    }
}
impl Drop for Nintendo {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn friend(id: &str) -> Friend {
        Friend {
            id: id.into(),
            name: id.into(),
            state: "PLAYING".into(),
            game: Some("Same game".into()),
            cover_url: None,
        }
    }
    #[test]
    fn profile_switch_preserves_timer_and_expiration_clears_presence() {
        let mut n = Nintendo::default();
        n.accept(Ok(vec![friend("a"), friend("b")]));
        n.select(Some("a"));
        n.started_at = 123;
        n.select(Some("b"));
        assert_eq!(n.started_at, 123);
        n.last_success = Some(Instant::now() - Duration::from_secs(91));
        n.select(Some("b"));
        assert!(n.game.is_none());
    }
    #[test]
    fn disable_discards_late_response() {
        let mut n = Nintendo::default();
        let (tx, rx) = mpsc::channel();
        n.pending = Some(rx);
        n.poll(false, &mut String::new(), false);
        assert!(tx.send(Ok(vec![friend("a")])).is_err());
        n.poll(true, &mut String::new(), false);
        assert!(n.game.is_none());
    }
    #[test]
    fn cancelled_install_cannot_overwrite_status() {
        let mut n = Nintendo::default();
        let (tx, rx) = mpsc::channel();
        n.install = Some(rx);
        n.cancel();
        assert!(tx.send(Update::Finished(Ok(()))).is_err());
        n.poll(false, &mut String::new(), false);
        assert!(matches!(n.status, ComponentStatus::Cancelled));
    }
}
