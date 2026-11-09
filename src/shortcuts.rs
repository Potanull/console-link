use eframe::egui::{self, Key, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Overlay,
    Escape,
    Fullscreen,
    CycleMode,
    Screenshot,
    Mute,
    Louder,
    Quieter,
    Stop,
}

pub fn action_for_key(key: Key, modifiers: Modifiers, typing: bool) -> Option<Action> {
    let primary = modifiers.command || modifiers.ctrl || modifiers.mac_cmd;
    match key {
        Key::F1 if !modifiers.alt && !primary => Some(Action::Overlay),
        Key::Escape => Some(Action::Escape),
        Key::F11 => Some(Action::Fullscreen),
        Key::F10 => Some(Action::CycleMode),
        Key::F8 => Some(Action::Screenshot),
        Key::Enter if primary && modifiers.shift => Some(Action::Stop),
        Key::Enter if primary || modifiers.alt => Some(Action::Fullscreen),
        Key::S if primary && modifiers.shift => Some(Action::Screenshot),
        Key::M if !typing && !primary && !modifiers.alt => Some(Action::Mute),
        Key::Plus | Key::Equals if !typing && !primary && !modifiers.alt => Some(Action::Louder),
        Key::Minus if !typing && !primary && !modifiers.alt => Some(Action::Quieter),
        _ => None,
    }
}

pub fn pressed(ctx: &egui::Context) -> Vec<Action> {
    let typing = ctx.wants_keyboard_input();
    ctx.input(|input| {
        input
            .events
            .iter()
            .filter_map(|event| match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } => action_for_key(*key, *modifiers, typing),
                _ => None,
            })
            .collect()
    })
}

pub const HELP: &[(&str, &str)] = &[
    ("F1", "Боковое меню"),
    ("F11 / Alt+Enter / ⌘Enter", "Полный экран"),
    ("F10", "Сменить режим окна"),
    ("F8 / Ctrl+Shift+S / ⌘⇧S", "Скриншот"),
    ("M", "Включить или выключить звук"),
    ("+ / -", "Громкость на 5%"),
    ("Esc", "Закрыть меню / выйти из полного экрана"),
    ("Ctrl+Shift+Enter / ⌘⇧Enter", "Завершить сеанс"),
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typing_game_names_does_not_mute_or_change_volume() {
        assert_eq!(action_for_key(Key::M, Modifiers::NONE, true), None);
        assert_eq!(action_for_key(Key::Plus, Modifiers::NONE, true), None);
        assert_eq!(
            action_for_key(Key::M, Modifiers::NONE, false),
            Some(Action::Mute)
        );
    }
    #[test]
    fn fullscreen_and_stop_chords_are_distinct() {
        assert_eq!(
            action_for_key(Key::Enter, Modifiers::ALT, false),
            Some(Action::Fullscreen)
        );
        let primary = Modifiers {
            command: true,
            ctrl: true,
            ..Modifiers::NONE
        };
        assert_eq!(
            action_for_key(Key::Enter, primary, false),
            Some(Action::Fullscreen)
        );
        assert_eq!(
            action_for_key(
                Key::Enter,
                Modifiers {
                    shift: true,
                    ..primary
                },
                false
            ),
            Some(Action::Stop)
        );
    }
}
