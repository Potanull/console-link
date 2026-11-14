use super::*;
use console_link::nintendo::{self, Friend};
use std::sync::mpsc;

#[derive(Default)]
pub(super) struct NintendoState {
    friends: Vec<Friend>,
    pending: Option<mpsc::Receiver<Result<Vec<Friend>, String>>>,
    last_attempt: Option<Instant>,
    last_success: Option<Instant>,
    error: Option<String>,
    game: Option<String>,
    started_at: i64,
    components: super::nintendo_install::ComponentInstall,
}
impl NintendoState {
    pub(super) fn preview(&mut self) {
        self.accept(Ok(vec![Friend {
            id: "0123456789abcdef".into(),
            name: "Nintendo Player".into(),
            state: "PLAYING".into(),
            game: Some("Example Adventure".into()),
            cover_url: None,
        }]));
        self.select_profile(Some("0123456789abcdef"));
    }
    fn set_game(&mut self, game: Option<String>) {
        if self.game != game {
            self.game = game;
            self.started_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|t| t.as_secs() as i64)
                .unwrap_or(0);
        }
    }
    fn select_profile(&mut self, id: Option<&str>) {
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
                self.error = None;
                self.last_success = Some(Instant::now());
            }
            Err(error) => {
                self.friends.clear();
                self.last_success = None;
                self.error = Some(error);
                self.set_game(None);
            }
        }
    }
}
impl ConsoleApp {
    pub(super) fn game_name(&self) -> &str {
        if self.settings.nintendo_enabled {
            self.nintendo.game.as_deref().unwrap_or("")
        } else {
            &self.settings.game
        }
    }
    pub(super) fn nintendo_cover(&self) -> Option<&str> {
        self.nintendo.game.as_ref()?;
        self.nintendo
            .friends
            .iter()
            .find(|f| f.id == self.settings.nintendo_friend_id)?
            .cover_url
            .as_deref()
    }
    pub(super) fn game_started_at(&self) -> i64 {
        if self.settings.nintendo_enabled {
            self.nintendo.started_at
        } else {
            self.started_unix
        }
    }
    pub(super) fn poll_nintendo(&mut self, ctx: &egui::Context) {
        if self.nintendo.components.poll() {
            self.nintendo.last_attempt = None;
        }
        if self.snapshot_path.is_some() && !std::env::args().any(|a| a == "--ui-nintendo-live") {
            return;
        }
        let before = self.game_name().to_owned();
        let received = self.nintendo.pending.as_ref().map(|rx| rx.try_recv());
        match received {
            Some(Ok(result)) => {
                self.nintendo.pending = None;
                if self.settings.nintendo_enabled {
                    self.nintendo.accept(result);
                }
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                self.nintendo.pending = None;
                self.nintendo
                    .accept(Err("Проверка Nintendo прервана.".into()));
            }
            _ => {}
        }
        if !self.settings.nintendo_enabled {
            self.nintendo.friends.clear();
            self.nintendo.last_success = None;
            self.nintendo.set_game(None);
        } else {
            if self.settings.nintendo_friend_id.is_empty() && self.nintendo.friends.len() == 1 {
                self.settings.nintendo_friend_id = self.nintendo.friends[0].id.clone();
                self.dirty = Some(Instant::now());
            }
            self.nintendo
                .select_profile(Some(&self.settings.nintendo_friend_id));
            if self.nintendo.pending.is_none()
                && self
                    .nintendo
                    .last_attempt
                    .is_none_or(|t| t.elapsed() >= Duration::from_secs(60))
            {
                let (tx, rx) = mpsc::channel();
                self.nintendo.pending = Some(rx);
                self.nintendo.last_attempt = Some(Instant::now());
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(nintendo::fetch());
                    ctx.request_repaint();
                });
            }
            ctx.request_repaint_after(Duration::from_secs(1));
        }
        if self.active.is_some() && self.game_name() != before {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(playback::game_window_title(
                self.game_name(),
            )));
        }
    }
    pub(super) fn nintendo_controls(&mut self, ui: &mut egui::Ui) {
        let before = (
            self.settings.nintendo_enabled,
            self.settings.nintendo_friend_id.clone(),
        );

        theme::toggle(
            ui,
            &mut self.settings.nintendo_enabled,
            "Получать текущую игру из Nintendo",
        );

        if self.settings.nintendo_enabled {
            self.nintendo.components.show(ui);
            ui.add_space(8.0);
            egui::ComboBox::from_id_salt("nintendo-friend")
                .selected_text(
                    self.nintendo
                        .friends
                        .iter()
                        .find(|f| f.id == self.settings.nintendo_friend_id)
                        .map(|f| f.name.as_str())
                        .unwrap_or("Выбери основной профиль"),
                )
                .show_ui(ui, |ui| {
                    for friend in &self.nintendo.friends {
                        ui.selectable_value(
                            &mut self.settings.nintendo_friend_id,
                            friend.id.clone(),
                            &friend.name,
                        );
                    }
                });
            if self.nintendo.pending.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Проверяем Nintendo…");
                });
            }
            if let Some(error) = &self.nintendo.error {
                theme::subtitle(ui, error);
            } else if self.nintendo.last_success.is_some() && self.nintendo.game.is_none() {
                theme::subtitle(
                    ui,
                    "Nintendo не сообщает текущую игру выбранного профиля. Статус Discord очищен.",
                );
            }
            theme::subtitle(ui, "Статус Nintendo обновляется раз в минуту.");
        }
        if before
            != (
                self.settings.nintendo_enabled,
                self.settings.nintendo_friend_id.clone(),
            )
        {
            self.nintendo.select_profile(
                self.settings
                    .nintendo_enabled
                    .then_some(self.settings.nintendo_friend_id.as_str()),
            );
            if self.active.is_some() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Title(
                    playback::game_window_title(self.game_name()),
                ));
            }
            self.dirty = Some(Instant::now());
            ui.ctx().request_repaint();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn switching_profiles_with_the_same_game_preserves_the_timer() {
        let mut state = NintendoState::default();
        let friend = Friend {
            id: "0123456789abcdef".into(),
            name: "First".into(),
            state: "PLAYING".into(),
            game: Some("Same Game".into()),
            cover_url: None,
        };
        let mut other = friend.clone();
        other.id = "fedcba9876543210".into();
        state.accept(Ok(vec![friend, other]));
        state.select_profile(Some("0123456789abcdef"));
        state.started_at = 123;
        state.select_profile(Some("fedcba9876543210"));
        assert_eq!(state.game.as_deref(), Some("Same Game"));
        assert_eq!(state.started_at, 123);
        state.select_profile(None);
        assert!(state.game.is_none());
    }

    #[test]
    fn expired_response_and_switching_target_clear_the_game() {
        let mut state = NintendoState::default();
        state.accept(Ok(vec![Friend {
            id: "0123456789abcdef".into(),
            name: "Test Player".into(),
            state: "PLAYING".into(),
            game: Some("Test Game".into()),
            cover_url: None,
        }]));
        state.select_profile(Some("0123456789abcdef"));
        assert_eq!(state.game.as_deref(), Some("Test Game"));
        state.select_profile(Some("fedcba9876543210"));
        assert!(state.game.is_none());
        state.last_success = Some(Instant::now() - Duration::from_secs(91));
        state.select_profile(Some("0123456789abcdef"));
        assert!(state.game.is_none());
    }
    #[test]
    fn title_timer_changes_only_with_title_and_error_clears_cached_friends() {
        let mut state = NintendoState::default();
        state.set_game(Some("Synthetic Game".into()));
        state.started_at = 123;
        state.set_game(Some("Synthetic Game".into()));
        assert_eq!(state.started_at, 123);
        state.accept(Ok(vec![Friend {
            id: "0123456789abcdef".into(),
            name: "Test Player".into(),
            state: "ONLINE".into(),
            game: Some("Synthetic Game".into()),
            cover_url: None,
        }]));
        state.accept(Err("Synthetic failure".into()));
        assert!(state.friends.is_empty());
        assert!(state.last_success.is_none());
        state.set_game(None);
        assert!(state.game.is_none());
    }
}
