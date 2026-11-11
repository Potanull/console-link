use serde::Deserialize;
use std::sync::OnceLock;
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Game {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub cover_url: Option<String>,
    pub game_url: Option<String>,
}

pub fn catalog() -> &'static [Game] {
    static GAMES: OnceLock<Vec<Game>> = OnceLock::new();
    GAMES.get_or_init(|| {
        serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/games.json"
        )))
        .expect("bundled game catalog must be valid")
    })
}

pub(super) fn normalize(text: &str) -> String {
    text.chars()
        .filter(|c| !matches!(c, '™' | '®' | '©'))
        .collect::<String>()
        .nfkd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

struct IndexedGame {
    aliases: Vec<String>,
}
fn index() -> &'static [IndexedGame] {
    static INDEX: OnceLock<Vec<IndexedGame>> = OnceLock::new();
    INDEX.get_or_init(|| {
        catalog()
            .iter()
            .map(|game| IndexedGame {
                aliases: std::iter::once(&game.name)
                    .chain(&game.aliases)
                    .map(|name| normalize(name))
                    .collect(),
            })
            .collect()
    })
}

pub fn find_game(name: &str) -> Option<Game> {
    let name = normalize(name);
    if name.is_empty() {
        return None;
    }
    index()
        .iter()
        .position(|entry| entry.aliases.contains(&name))
        .map(|i| catalog()[i].clone())
}

pub fn search_games(query: &str, limit: usize) -> Vec<Game> {
    let query = normalize(query);
    if query.is_empty() || limit == 0 {
        return Vec::new();
    }
    let mut results: Vec<_> = index()
        .iter()
        .enumerate()
        .filter_map(|(i, entry)| {
            entry
                .aliases
                .iter()
                .filter(|name| name.contains(&query))
                .map(|name| {
                    if *name == query {
                        0
                    } else if name.starts_with(&query) {
                        1
                    } else {
                        2
                    }
                })
                .min()
                .map(|rank| (rank, i))
        })
        .collect();
    results.sort_by_key(|&(rank, i)| (rank, &catalog()[i].name));
    results
        .into_iter()
        .take(limit.min(100))
        .map(|(_, i)| catalog()[i].clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manual_catalog_finds_titles_aliases_and_limits_results() {
        assert!(catalog().len() > 20000);
        assert!(find_game("Mario Kart 8 Deluxe").is_some());
        assert!(find_game("The Legend of Zelda: Breath of the Wild").is_some());
        assert!(find_game("").is_none());
        assert!(search_games("blaster master", 4).len() >= 3);
        assert!(search_games("zelda", 2).len() <= 2);
        assert!(search_games("", 10).is_empty());
        assert!(search_games("zelda", 0).is_empty());
        assert_eq!(normalize("Pokémon™"), "pokemon");
    }
}
