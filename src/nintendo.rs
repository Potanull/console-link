use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Friend {
    pub id: String,
    pub name: String,
    pub state: String,
    pub game: Option<String>,
    #[serde(default)]
    pub cover_url: Option<String>,
}

const MAX_RESPONSE: usize = 1024 * 1024;
const FETCH_ERROR: &str =
    "Не удалось получить статус Nintendo. Проверь интернет и вход во второй аккаунт nxapi.";

pub fn decode(bytes: &[u8]) -> Result<Vec<Friend>, String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Reply {
        ok: bool,
        #[serde(default)]
        friends: Vec<Friend>,
    }
    if bytes.len() > MAX_RESPONSE {
        return Err(FETCH_ERROR.into());
    }
    let reply: Reply = serde_json::from_slice(bytes).map_err(|_| FETCH_ERROR)?;
    if !reply.ok
        || reply.friends.len() > 300
        || reply.friends.iter().any(|f| {
            f.id.len() != 16
                || !f.id.bytes().all(|b| b.is_ascii_hexdigit())
                || f.name.chars().count() > 128
                || f.name.chars().any(char::is_control)
                || !matches!(
                    f.state.as_str(),
                    "ONLINE" | "PLAYING" | "INACTIVE" | "OFFLINE"
                )
                || f.cover_url.as_deref().is_some_and(|u| !valid_cover(u))
                || f.game.as_ref().is_some_and(|g| {
                    g.trim().is_empty()
                        || g.chars().count() > 128
                        || g.chars().any(char::is_control)
                })
        })
    {
        return Err(FETCH_ERROR.into());
    }
    Ok(reply.friends)
}

pub fn selected_game<'a>(friends: &'a [Friend], id: &str) -> Option<&'a str> {
    friends
        .iter()
        .find(|f| f.id == id && matches!(f.state.as_str(), "ONLINE" | "PLAYING"))?
        .game
        .as_deref()
}

fn valid_cover(value: &str) -> bool {
    value.len() <= 2048
        && url::Url::parse(value).is_ok_and(|u| {
            u.scheme() == "https"
                && u.username().is_empty()
                && u.password().is_none()
                && u.port().is_none()
                && matches!(
                    u.host_str(),
                    Some("atum-img-lp1.cdn.nintendo.net" | "img-eshop.cdn.nintendo.net")
                )
        })
}
fn find_helper(
    executable: &std::path::Path,
    local: &std::path::Path,
) -> Option<std::path::PathBuf> {
    executable
        .ancestors()
        .skip(1)
        .map(|p| p.join(".local/nintendo-helper"))
        .chain(std::iter::once(local.to_owned()))
        .find(|p| {
            p.join("node_modules/nxapi/bin/nxapi.js").is_file() && p.join("data/persist").is_dir()
        })
}

/// Run only on a worker thread. No shell, credential arguments, or raw SDK error logging.
pub fn fetch() -> Result<Vec<Friend>, String> {
    use std::{
        io::Read,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let local = directories::BaseDirs::new()
        .ok_or(FETCH_ERROR)?
        .data_local_dir()
        .join("ConsoleLink/nintendo-helper");
    let executable = std::env::current_exe().map_err(|_| FETCH_ERROR)?;
    let base = find_helper(&executable, &local)
        .ok_or("Локальный nxapi не найден. Проверь папку помощника Nintendo и сохранённый вход.")?;
    let mut command = Command::new("node");
    command
        .args([
            "-e",
            include_str!("../scripts/nintendo-presence.cjs"),
            "--",
            "--console-link-presence",
        ])
        .arg(base)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .spawn()
        .map_err(|_| "Не удалось запустить Node.js для nxapi. Проверь установку Node.js.")?;
    let stdout = child.stdout.take().ok_or(FETCH_ERROR)?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take((MAX_RESPONSE + 1) as u64)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let start = Instant::now();
    let successful = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if start.elapsed() < Duration::from_secs(50) => {
                std::thread::sleep(Duration::from_millis(100))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    let bytes = reader
        .join()
        .map_err(|_| FETCH_ERROR)?
        .map_err(|_| FETCH_ERROR)?;
    if !successful {
        return Err(FETCH_ERROR.into());
    }
    decode(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cover_urls_are_public_nintendo_https_only() {
        assert!(valid_cover(
            "https://atum-img-lp1.cdn.nintendo.net/example.jpg"
        ));
        for value in [
            "http://atum-img-lp1.cdn.nintendo.net/a",
            "https://user:password@atum-img-lp1.cdn.nintendo.net/a",
            "https://atum-img-lp1.cdn.nintendo.net.attacker.invalid/a",
            "https://localhost/a",
        ] {
            assert!(!valid_cover(value));
        }
    }
    #[test]
    fn repository_helper_works_when_system_appdata_is_redirected() {
        let root =
            std::env::temp_dir().join(format!("console-link-helper-test-{}", std::process::id()));
        let helper = root.join(".local/nintendo-helper");
        std::fs::create_dir_all(helper.join("node_modules/nxapi/bin")).unwrap();
        std::fs::create_dir_all(helper.join("data/persist")).unwrap();
        std::fs::write(helper.join("node_modules/nxapi/bin/nxapi.js"), "// fixture").unwrap();
        assert_eq!(
            find_helper(
                &root.join("target/release/app.exe"),
                &root.join("redirected-appdata")
            ),
            Some(helper)
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[ignore = "Requires the user's locally authenticated nxapi; never run in CI"]
    fn live_local_helper_returns_filtered_friends() {
        let friends = fetch().expect("Local Nintendo presence query failed");
        assert!(!friends.is_empty());
        assert!(friends.iter().all(|f| f.id.len() == 16));
    }
    #[test]
    fn protocol_rejects_errors_malformed_and_private_fields() {
        assert!(decode(br#"{"ok":false}"#).is_err());
        assert!(decode(br#"{"ok":true,"friends":[{"id":"0123456789abcdef","name":"Player","state":"ONLINE","game":"Test Game","token":"secret"}]}"#).is_err());
        assert!(decode(br#"{"ok":true,"friends":[{"id":"bad","name":"Player","state":"ONLINE","game":"Test Game"}]}"#).is_err());
        assert!(decode(&vec![b' '; MAX_RESPONSE + 1]).is_err());
    }
    #[test]
    fn selection_never_guesses_between_friends_or_shows_offline_game() {
        let friends = decode(br#"{"ok":true,"friends":[{"id":"0123456789abcdef","name":"Player","state":"ONLINE","game":"Test Game"},{"id":"fedcba9876543210","name":"Friend","state":"INACTIVE","game":"Old Game"}]}"#).unwrap();
        assert_eq!(selected_game(&friends, ""), None);
        assert_eq!(
            selected_game(&friends, "0123456789abcdef"),
            Some("Test Game")
        );
        assert_eq!(selected_game(&friends, "fedcba9876543210"), None);
        assert_eq!(selected_game(&[], "0123456789abcdef"), None);
    }
}
