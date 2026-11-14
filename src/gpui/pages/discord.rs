use super::*;

impl Desktop {
    pub(super) fn discord_page(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let account = self.runtime.discord_status();
        let linked = matches!(&account, AuthStatus::Connected(_));
        let mut identity = card("Discord", "Твой профиль, название игры и обложка.");
        match account {
            AuthStatus::Connected(account) => {
                let mut profile = div().flex().w_full().flex_shrink_0().items_center().gap_4();
                if let Some(avatar) = self.artwork_image(account.avatar_url, window, cx) {
                    profile = profile.child(
                        img(avatar)
                            .size(px(56.))
                            .flex_shrink_0()
                            .rounded_full()
                            .object_fit(gpui::ObjectFit::Cover),
                    );
                } else {
                    profile = profile.child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(56.))
                            .flex_shrink_0()
                            .rounded_full()
                            .bg(rgb(theme::HOVER))
                            .child("D"),
                    );
                }
                identity = identity
                    .child(
                        profile.child(
                            column()
                                .flex_1()
                                .gap_1()
                                .child(text(account.display_name))
                                .child(hint("Аккаунт Discord подключен")),
                        ),
                    )
                    .child(action_button(
                        "discord-logout",
                        "Отключить аккаунт",
                        false,
                        cx.listener(|s, _, _, cx| {
                            s.runtime.discord_logout();
                            cx.notify();
                        }),
                    ));
            }
            AuthStatus::Connecting => {
                identity = identity
                    .child(text("Подтверди вход в Discord"))
                    .child(hint("После подтверждения вернись в приложение."))
                    .child(action_button(
                        "discord-cancel",
                        "Отменить вход",
                        false,
                        cx.listener(|s, _, _, cx| {
                            s.runtime.discord_cancel();
                            cx.notify();
                        }),
                    ))
            }
            status => {
                if let AuthStatus::Error(error) = status {
                    identity = identity.child(hint(error));
                }
                identity = identity
                    .child(action_button(
                        "discord-login",
                        "Подключить Discord",
                        true,
                        cx.listener(|s, _, _, cx| {
                            s.runtime.discord_login();
                            cx.notify();
                        }),
                    ))
                    .child(action_button(
                        "discord-forget",
                        "Забыть сохраненный вход",
                        false,
                        cx.listener(|s, _, _, cx| {
                            s.runtime.discord_logout();
                            cx.notify();
                        }),
                    ));
            }
        }
        let game_name = self.runtime.game_name().to_owned();
        let has_game = !game_name.is_empty();
        let cover = self.runtime.cover_url();
        let mut game = div().flex().w_full().flex_shrink_0().items_center().gap_4();
        if let Some(image) = self.artwork_image(cover, window, cx) {
            game = game.child(
                img(image)
                    .size(px(76.))
                    .flex_shrink_0()
                    .rounded_lg()
                    .object_fit(gpui::ObjectFit::Cover),
            );
        }
        let mut game_details = column()
            .flex_1()
            .gap_1()
            .child(text(if game_name.is_empty() {
                "Игра пока не определена".to_owned()
            } else {
                game_name
            }))
            .child(hint("Nintendo Switch"));
        if has_game && (self.runtime.active() || self.runtime.settings.nintendo_enabled) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |t| t.as_secs() as i64);
            let minutes = now.saturating_sub(self.runtime.game_started_at()).max(0) / 60;
            game_details = game_details.child(hint(format!("В игре {minutes} мин")));
        }
        game = game.child(game_details);
        let presence = if !self.runtime.settings.discord_enabled {
            "Показ активности выключен.".to_owned()
        } else if !linked {
            "Подключи аккаунт для игрового статуса.".to_owned()
        } else if self.runtime.settings.nintendo_enabled && !has_game {
            "Ожидаем текущую игру из Nintendo.".to_owned()
        } else if !self.runtime.active() && !self.runtime.settings.nintendo_enabled {
            "Статус появится при подключении консоли.".to_owned()
        } else {
            friendly_presence(&self.runtime.presence_status())
        };
        let activity=card("Сейчас играешь","Название обновляется независимо от изображения карты захвата.")
            .child(game)
            .child(switch(cx,"discord-enabled","Показывать игру в Discord",self.runtime.settings.discord_enabled,|s,v|s.runtime.settings.discord_enabled=v))
            .child(hint(presence))
            .child(hint("Для демонстрации выбери игровое окно Console Link в настольном Discord. Передача изображения и звука управляется Discord."));
        let mut nintendo = card(
            "Определение игры Nintendo",
            "Получаем текущую игру выбранного профиля Nintendo.",
        )
        .child(switch(
            cx,
            "nintendo-enabled",
            "Определять игру автоматически",
            self.runtime.settings.nintendo_enabled,
            |s, v| s.runtime.settings.nintendo_enabled = v,
        ));
        let components = self.runtime.components_status();
        let component_text = match components {
            ComponentStatus::Unknown => "Проверка компонентов еще не выполнена.".to_owned(),
            ComponentStatus::Missing => "Компоненты Nintendo не установлены.".to_owned(),
            ComponentStatus::Installing(stage) => stage.label().to_owned(),
            ComponentStatus::Ready => "Компоненты Nintendo установлены.".to_owned(),
            ComponentStatus::Failed(error) => error.clone(),
            ComponentStatus::Cancelled => "Установка отменена.".to_owned(),
        };
        nintendo = nintendo.child(hint(component_text));
        if matches!(components, ComponentStatus::Installing(_)) {
            nintendo = nintendo.child(action_button(
                "nintendo-cancel-install",
                "Отменить установку",
                false,
                cx.listener(|s, _, _, cx| {
                    s.runtime.cancel_install();
                    cx.notify();
                }),
            ));
        } else {
            nintendo = nintendo
                .child(action_button(
                    "nintendo-install",
                    if matches!(components, ComponentStatus::Ready) {
                        "Восстановить компоненты"
                    } else {
                        "Установить компоненты"
                    },
                    false,
                    cx.listener(|s, _, _, cx| {
                        s.runtime.install_components();
                        cx.notify();
                    }),
                ))
                .child(action_button(
                    "nintendo-check",
                    "Проверить компоненты",
                    false,
                    cx.listener(|s, _, _, cx| {
                        s.runtime.check_components();
                        cx.notify();
                    }),
                ));
        }
        nintendo=nintendo.child(hint("Node.js и nxapi устанавливаются только для Console Link. Системный Node не меняется. Вход в Nintendo настраивается отдельно."));
        let profiles = self.runtime.nintendo_friends();
        if !profiles.is_empty() {
            let selected = profiles
                .iter()
                .find(|friend| friend.id == self.runtime.settings.nintendo_friend_id)
                .map(|friend| friend.name.clone())
                .unwrap_or_else(|| "Выбери профиль".into());
            let picker = self.select_box(
                Selection::NintendoProfile,
                "nintendo-profile",
                selected,
                profiles
                    .iter()
                    .map(|friend| {
                        (
                            friend.id.clone(),
                            friend.name.clone(),
                            friend.id == self.runtime.settings.nintendo_friend_id,
                        )
                    })
                    .collect(),
                cx,
                |s, id, _, _| s.runtime.select_profile(id),
            );
            nintendo = nintendo.child(setting_control("Профиль", picker));
        }
        if self.runtime.nintendo_pending() {
            nintendo = nintendo.child(hint("Проверяем текущую игру…"));
        }
        if let Some(error) = self.runtime.nintendo_error() {
            nintendo = nintendo.child(hint(error.to_owned()));
        }
        nintendo = nintendo
            .child(action_button(
                "nintendo-refresh",
                "Обновить статус Nintendo",
                false,
                cx.listener(|s, _, _, cx| {
                    s.runtime.nintendo_refresh();
                    cx.notify();
                }),
            ))
            .child(hint(
                "Обновление раз в минуту. Если Nintendo не сообщает игру, статус очищается.",
            ));
        let mut manual = card(
            "Выбрать название вручную",
            "Ручной выбор отключает автоматическое определение Nintendo.",
        )
        .child(caption("Название игры"))
        .child(self.game_input.clone());
        if self.game_query.chars().count() >= 2 {
            for (i, entry) in self
                .runtime
                .search_games(&self.game_query)
                .into_iter()
                .take(6)
                .enumerate()
            {
                let name = entry.name;
                manual = manual.child(widgets::button(
                    ("game-result", i),
                    name.clone(),
                    false,
                    cx.listener(move |s, _, window, cx| s.choose_game(name.clone(), window, cx)),
                ));
            }
        }
        if !self.game_query.trim().is_empty() {
            manual = manual.child(action_button(
                "manual-game",
                "Использовать введенное название",
                true,
                cx.listener(|s, _, window, cx| {
                    let name = s.game_query.trim().to_owned();
                    s.choose_game(name, window, cx);
                }),
            ));
        }
        column()
            .w_full()
            .gap_6()
            .child(identity)
            .child(activity)
            .child(nintendo)
            .child(manual)
            .into_any_element()
    }
}
