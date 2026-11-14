use super::*;

impl Runtime {
    pub fn game_name(&self) -> &str {
        if self.settings.nintendo_enabled {
            self.nintendo.game.as_deref().unwrap_or("")
        } else {
            &self.settings.game
        }
    }
    pub fn game_started_at(&self) -> i64 {
        if self.settings.nintendo_enabled {
            self.nintendo.started_at
        } else {
            self.started_unix
        }
    }
    pub fn current_game(&mut self) -> Option<Game> {
        if self.metadata_name != self.game_name() {
            self.metadata_name = self.game_name().to_owned();
            self.metadata = catalog::find_game(self.game_name());
        }
        let mut game = self.metadata.clone();
        if self.settings.nintendo_enabled && !self.game_name().is_empty() {
            let entry = game.get_or_insert_with(|| Game {
                id: String::new(),
                name: self.game_name().into(),
                aliases: vec![],
                cover_url: None,
                game_url: None,
            });
            if let Some(cover) = self
                .nintendo
                .friends
                .iter()
                .find(|f| f.id == self.settings.nintendo_friend_id)
                .and_then(|f| f.cover_url.clone())
            {
                entry.cover_url = Some(cover);
            }
        }
        game
    }
    pub fn cover_url(&mut self) -> Option<String> {
        self.current_game().and_then(|g| g.cover_url)
    }
    pub fn search_games(&self, query: &str) -> Vec<Game> {
        catalog::search_games(query, 20)
    }
    pub fn choose_game(&mut self, game: String) {
        let before = self.game_name().to_owned();
        let previous_started = self.game_started_at();
        self.settings.nintendo_enabled = false;
        self.settings.game = game.chars().filter(|c| !c.is_control()).take(128).collect();
        self.started_unix = if before == self.game_name() {
            previous_started
        } else {
            unix_now()
        };
        self.previous = self.settings.clone();
        self.dirty = Some(Instant::now());
    }
    pub(super) fn presence(&mut self, linked: bool) -> Option<Presence> {
        if !(linked
            && self.settings.discord_enabled
            && (self.active() || self.settings.nintendo_enabled)
            && !self.game_name().is_empty())
        {
            return None;
        }
        let game = self.current_game();
        Some(Presence {
            application_id: DEFAULT_APPLICATION_ID.into(),
            game: self
                .game_name()
                .chars()
                .filter(|c| !c.is_control())
                .take(128)
                .collect(),
            started_at: self.game_started_at(),
            cover_url: game.as_ref().and_then(|g| g.cover_url.clone()),
            game_url: game.and_then(|g| g.game_url),
        })
    }
    pub fn discord_status(&self) -> AuthStatus {
        self.auth
            .as_ref()
            .map_or(AuthStatus::Disconnected, DiscordAuth::status)
    }
    pub fn discord_account(&self) -> Option<DiscordAccount> {
        self.auth.as_ref().and_then(DiscordAuth::account)
    }
    pub fn presence_status(&self) -> String {
        self.discord
            .as_ref()
            .map_or_else(|| "Отключено".into(), DiscordWorker::status)
    }
    pub fn discord_login(&mut self) {
        if self.persistent {
            if self.auth.is_none() {
                self.auth = Some(DiscordAuth::new(DEFAULT_APPLICATION_ID.into()));
                self.auth_completion = 0;
            }
            if let Some(a) = &self.auth {
                a.connect();
            }
        }
    }
    pub fn discord_logout(&mut self) {
        if let Some(a) = &self.auth {
            a.disconnect();
        }
        self.settings.discord_enabled = false;
        self.changed();
        if let Some(d) = &self.discord {
            d.set(None);
        }
    }
    /// Cancels the pending login without deleting the saved account from the vault.
    pub fn discord_cancel(&mut self) {
        self.auth = None;
        self.auth_completion = 0;
        if let Some(d) = &self.discord {
            d.set(None);
            d.require_account(None);
        }
    }
    pub fn nintendo_friends(&self) -> &[crate::nintendo::Friend] {
        &self.nintendo.friends
    }
    pub fn nintendo_error(&self) -> Option<&str> {
        self.nintendo.error.as_deref()
    }
    pub fn nintendo_pending(&self) -> bool {
        self.nintendo.pending()
    }
    pub fn nintendo_refresh(&mut self) {
        self.nintendo.refresh();
    }
    pub fn select_profile(&mut self, id: String) {
        self.settings.nintendo_friend_id = id;
        self.changed();
    }
    pub fn components_status(&self) -> &ComponentStatus {
        &self.nintendo.status
    }
    pub fn check_components(&mut self) {
        if self.persistent {
            self.nintendo.check_components();
        }
    }
    pub fn install_components(&mut self) {
        if self.persistent {
            self.nintendo.install();
        }
    }
    pub fn cancel_install(&mut self) {
        self.nintendo.cancel();
    }
}
