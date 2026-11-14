pub mod installation;
mod runtime_config;

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
    helper_locations(executable, local).find(|p| {
        p.join("node_modules/nxapi/bin/nxapi.js").is_file() && p.join("data/persist").is_dir()
    })
}

fn helper_locations<'a>(
    executable: &'a std::path::Path,
    local: &'a std::path::Path,
) -> impl Iterator<Item = std::path::PathBuf> + 'a {
    executable
        .ancestors()
        .skip(1)
        .map(|p| p.join(".local/nintendo-helper"))
        .chain(std::iter::once(local.to_owned()))
}

fn data_directory(
    executable: &std::path::Path,
    local: &std::path::Path,
    legacy: Option<&std::path::Path>,
) -> std::path::PathBuf {
    legacy
        .map(std::path::Path::to_owned)
        .or_else(|| {
            helper_locations(executable, local).find(|base| base.join("data/persist").is_dir())
        })
        .unwrap_or_else(|| local.to_owned())
        .join("data")
}

pub fn helper_directory() -> Result<std::path::PathBuf, String> {
    directories::BaseDirs::new()
        .map(|dirs| dirs.data_local_dir().join("ConsoleLink/nintendo-helper"))
        .ok_or_else(|| "Каталог компонентов Nintendo недоступен.".into())
}

#[cfg(any(target_os = "macos", test))]
fn resolve_macos_node(
    path: Option<&std::ffi::OsStr>,
    executable: impl Fn(&std::path::Path) -> bool,
) -> std::path::PathBuf {
    path.into_iter()
        .flat_map(std::env::split_paths)
        .map(|directory| directory.join("node"))
        .chain(["/opt/homebrew/bin/node", "/usr/local/bin/node"].map(std::path::PathBuf::from))
        .find(|candidate| executable(candidate))
        .unwrap_or_else(|| "node".into())
}

#[cfg(target_os = "macos")]
fn executable_file(candidate: &std::path::Path) -> bool {
    use std::{
        ffi::{CString, c_char, c_int},
        os::unix::ffi::OsStrExt,
    };
    unsafe extern "C" {
        fn access(path: *const c_char, mode: c_int) -> c_int;
    }
    // Check this caller's execute access, including ACLs, not another user's mode bit.
    candidate.is_file()
        && CString::new(candidate.as_os_str().as_bytes()).is_ok_and(|path| {
            // SAFETY: CString supplies a valid NUL-terminated path for this call; 1 is X_OK.
            unsafe { access(path.as_ptr(), 1) == 0 }
        })
}

fn node_program() -> std::path::PathBuf {
    #[cfg(target_os = "macos")]
    {
        // Finder's minimal PATH omits standard Homebrew and Node installers.
        resolve_macos_node(std::env::var_os("PATH").as_deref(), executable_file)
    }
    #[cfg(not(target_os = "macos"))]
    {
        "node".into()
    }
}

/// Run only on a worker thread. No shell, credential arguments, or raw SDK error logging.
pub fn fetch() -> Result<Vec<Friend>, String> {
    use std::{
        io::Read,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let local = helper_directory()?;
    let executable = std::env::current_exe().map_err(|_| FETCH_ERROR)?;
    let legacy = find_helper(&executable, &local);
    let data = data_directory(&executable, &local, legacy.as_deref());
    let (node, base) = if let Some(managed) = installation::installed(&local) {
        (managed.node, managed.package_directory)
    } else {
        (
            node_program(),
            legacy.ok_or(
                "Компоненты Nintendo не установлены. Нажми «Установить компоненты Nintendo».",
            )?,
        )
    };
    let mut command = Command::new(node);
    command
        .current_dir(&base)
        .env_remove("NODE_OPTIONS")
        .env_remove("NODE_PATH")
        .env("CONSOLE_LINK_VERSION", env!("CARGO_PKG_VERSION"))
        .args([
            "-e",
            include_str!("../scripts/nintendo-presence.cjs"),
            "--",
            "--console-link-presence",
        ])
        .arg(base)
        .arg(data)
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
    fn macos_node_resolver_preserves_path_before_installer_fallbacks() {
        use std::path::Path;
        let custom_path = std::env::join_paths(["/custom/bin", "/usr/bin", "/bin"]).unwrap();
        let finder_path = std::env::join_paths(["/usr/bin", "/bin"]).unwrap();
        let chosen = resolve_macos_node(Some(&custom_path), |candidate| {
            [
                Path::new("/custom/bin/node"),
                Path::new("/opt/homebrew/bin/node"),
            ]
            .contains(&candidate)
        });
        assert_eq!(chosen, Path::new("/custom/bin/node"));
        for available in ["/opt/homebrew/bin/node", "/usr/local/bin/node"] {
            assert_eq!(
                resolve_macos_node(Some(&finder_path), |candidate| candidate
                    == Path::new(available)),
                Path::new(available)
            );
        }
        assert_eq!(
            resolve_macos_node(Some(&finder_path), |candidate| {
                [
                    Path::new("/opt/homebrew/bin/node"),
                    Path::new("/usr/local/bin/node"),
                ]
                .contains(&candidate)
            }),
            Path::new("/opt/homebrew/bin/node")
        );
        assert_eq!(resolve_macos_node(None, |_| false), Path::new("node"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_node_candidates_require_caller_execute_access_and_regular_file() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "console-link-node-permission-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let node = root.join("node");
        std::fs::write(&node, "").unwrap();
        std::fs::set_permissions(&node, std::fs::Permissions::from_mode(0o610)).unwrap();
        assert!(
            !executable_file(&node),
            "Owner lacks execute access even when the group has it"
        );
        std::fs::set_permissions(&node, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(executable_file(&node));
        assert!(
            !executable_file(&root),
            "An executable directory is not Node"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn managed_component_lock_contains_only_integrity_pinned_registry_packages() {
        let package: serde_json::Value =
            serde_json::from_str(include_str!("../packaging/nintendo/package.json")).unwrap();
        assert_eq!(
            package["dependencies"]["nxapi"],
            runtime_config::NXAPI_VERSION
        );
        let lock: serde_json::Value =
            serde_json::from_str(include_str!("../packaging/nintendo/package-lock.json")).unwrap();
        for (name, package) in lock["packages"].as_object().unwrap() {
            if name.is_empty() {
                continue;
            }
            assert!(
                package["resolved"]
                    .as_str()
                    .is_some_and(|url| url.starts_with("https://registry.npmjs.org/")),
                "{name}"
            );
            assert!(
                package["integrity"]
                    .as_str()
                    .is_some_and(|value| value.starts_with("sha512-") && value.len() > 16),
                "{name}"
            );
        }
    }

    #[test]
    fn installing_runtime_does_not_change_an_existing_account_location() {
        let root = std::env::temp_dir().join(format!(
            "console-link-data-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let previous = root.join("project/.local/nintendo-helper");
        std::fs::create_dir_all(previous.join("data/persist")).unwrap();
        let executable = root.join("project/target/release/console-link");
        let local = root.join("application-data");
        assert_eq!(
            data_directory(&executable, &local, None),
            previous.join("data")
        );
        let selected_legacy = root.join("previously-selected");
        assert_eq!(
            data_directory(&executable, &local, Some(&selected_legacy)),
            selected_legacy.join("data")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

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
