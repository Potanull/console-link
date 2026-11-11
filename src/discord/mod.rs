use crate::config::discord as config;

pub mod auth;
mod transport;

use transport::{Transport, decode_header};

use crossbeam_channel::{Receiver, Sender, bounded};
use discord_rich_presence::{DiscordIpc, activity, error::Error};
use serde_json::{Value, json};
#[cfg(all(test, unix))]
use std::io::{Read, Write};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

fn valid_id(id: &str) -> bool {
    (16..=22).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Presence {
    pub application_id: String,
    pub game: String,
    pub started_at: i64,
    pub cover_url: Option<String>,
    pub game_url: Option<String>,
}

impl Presence {
    pub fn valid(&self) -> bool {
        valid_id(&self.application_id)
            && !self.game.trim().is_empty()
            && self.game.chars().count() <= 128
            && !self.game.chars().any(char::is_control)
            && self.cover_url.as_deref().is_none_or(public_https_url)
            && self.game_url.as_deref().is_none_or(public_https_url)
    }
}

fn public_https_url(value: &str) -> bool {
    value.len() <= 2048
        && url::Url::parse(value).is_ok_and(|url| {
            url.scheme() == "https"
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
        })
}

fn game_activity(presence: &Presence) -> activity::Activity<'_> {
    let mut result = activity::Activity::new()
        .name(presence.game.as_str())
        .activity_type(activity::ActivityType::Playing)
        .details(presence.game.as_str())
        .state("Nintendo Switch · Console Link")
        .status_display_type(activity::StatusDisplayType::Details)
        .timestamps(activity::Timestamps::new().start(presence.started_at));
    let mut assets = activity::Assets::new()
        .small_image(config::ICON_URL)
        .small_text("Console Link");
    if let Some(cover) = presence.cover_url.as_deref() {
        assets = assets.large_image(cover).large_text(presence.game.as_str());
    }
    result = result.assets(assets);
    if let Some(page) = presence.game_url.as_deref() {
        result = result.buttons(vec![activity::Button::new("Об игре", page)]);
    }
    result
}

pub struct DiscordWorker {
    desired: Arc<Mutex<Option<Presence>>>,
    status: Arc<Mutex<String>>,
    wake: Sender<()>,
    stop: Sender<()>,
    account_requirement: Arc<Mutex<AccountRequirement>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum AccountRequirement {
    #[default]
    Legacy,
    SignedOut,
    User(String),
}

fn account_allowed(requirement: &AccountRequirement, desktop_user_id: Option<&str>) -> bool {
    match requirement {
        AccountRequirement::Legacy => true,
        AccountRequirement::SignedOut => false,
        AccountRequirement::User(expected) => desktop_user_id == Some(expected.as_str()),
    }
}

impl Default for DiscordWorker {
    fn default() -> Self {
        Self::new()
    }
}

impl DiscordWorker {
    pub fn new() -> Self {
        let desired = Arc::new(Mutex::new(None));
        let status = Arc::new(Mutex::new("Off".into()));
        let (wake, wake_rx) = bounded(1);
        let (stop, stop_rx) = bounded(1);
        let account_requirement = Arc::new(Mutex::new(AccountRequirement::Legacy));
        let worker_requirement = Arc::clone(&account_requirement);
        let (desired_clone, status_clone) = (Arc::clone(&desired), Arc::clone(&status));
        std::thread::spawn(move || {
            run(
                desired_clone,
                status_clone,
                wake_rx,
                stop_rx,
                worker_requirement,
            )
        });
        Self {
            desired,
            status,
            wake,
            stop,
            account_requirement,
        }
    }

    pub fn set(&self, presence: Option<Presence>) {
        let mut desired = self.desired.lock().unwrap_or_else(|e| e.into_inner());
        if *desired != presence {
            *desired = presence;
            let _ = self.wake.try_send(());
        }
    }

    pub fn status(&self) -> String {
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Enables account-checked presence. `None` signs out and clears published activity.
    /// Call once during app startup, then whenever the linked OAuth identity changes.
    pub fn require_account(&self, account_id: Option<String>) {
        let next = account_id
            .map(AccountRequirement::User)
            .unwrap_or(AccountRequirement::SignedOut);
        let mut requirement = self
            .account_requirement
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if *requirement != next {
            *requirement = next;
            let _ = self.wake.try_send(());
        }
    }
}

impl Drop for DiscordWorker {
    fn drop(&mut self) {
        let _ = self.stop.try_send(());
    }
}

fn run(
    desired: Arc<Mutex<Option<Presence>>>,
    status: Arc<Mutex<String>>,
    wake: Receiver<()>,
    stop: Receiver<()>,
    account_requirement: Arc<Mutex<AccountRequirement>>,
) {
    let mut client: Option<Client> = None;
    let mut sent: Option<Presence> = None;
    let mut last_try = Instant::now() - Duration::from_secs(10);
    let mut last_requirement = AccountRequirement::Legacy;
    loop {
        if stop.try_recv().is_ok() {
            if let Some(mut client) = client {
                let _ = client.clear_activity();
                let _ = client.close();
            }
            break;
        }
        let wanted = desired.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let requirement = account_requirement
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if requirement != last_requirement {
            if let Some(mut old) = client.take() {
                let _ = old.clear_activity();
                let _ = old.close();
            }
            sent = None;
            last_requirement = requirement.clone();
            last_try = Instant::now() - Duration::from_secs(10);
        }
        let invalid = wanted.as_ref().is_some_and(|p| !p.valid());
        let signed_out = requirement == AccountRequirement::SignedOut;
        let wanted = wanted.filter(|presence| presence.valid() && !signed_out);
        if wanted.is_none() {
            if let Some(mut old) = client.take() {
                let _ = old.clear_activity();
                let _ = old.close();
            }
            sent = None;
            *status.lock().unwrap_or_else(|e| e.into_inner()) = if signed_out {
                "Link your Discord account to share activity".into()
            } else if invalid {
                "Enter a Discord Application ID and a game name".into()
            } else {
                "Off".into()
            };
        } else if let Some(presence) = wanted
            .as_ref()
            .filter(|_| wanted != sent || last_try.elapsed() >= config::PUBLISH_INTERVAL)
        {
            if client
                .as_ref()
                .is_some_and(|c| c.id != presence.application_id)
            {
                if let Some(mut old) = client.take() {
                    let _ = old.clear_activity();
                    let _ = old.close();
                }
                sent = None;
            }
            if last_try.elapsed() >= config::RECONNECT_INTERVAL {
                last_try = Instant::now();
                let mut account_mismatch = false;
                let result = (|| -> Result<NameConfirmation, Error> {
                    if client.is_none() {
                        let mut new = Client {
                            id: presence.application_id.clone(),
                            transport: None,
                            user_id: None,
                        };
                        new.connect()?;
                        client = Some(new);
                    }
                    let connected = client.as_mut().expect("connected");
                    let current_requirement = account_requirement
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .clone();
                    if !account_allowed(&current_requirement, connected.user_id.as_deref()) {
                        account_mismatch = true;
                        return Err(Error::IPCConnectionFailed);
                    }
                    connected.publish_activity(presence)
                })();
                match result {
                    Ok(confirmation) => {
                        sent = wanted;
                        *status.lock().unwrap_or_else(|e| e.into_inner()) = match confirmation {
                            NameConfirmation::Exact => "Activity sent to desktop Discord",
                            NameConfirmation::Ignored => {
                                "Activity accepted but Discord did not use the selected game name"
                            }
                            NameConfirmation::Unknown => {
                                "Activity accepted by desktop Discord (game name not confirmed)"
                            }
                        }
                        .into();
                    }
                    Err(_) => {
                        client = None;
                        sent = None;
                        *status.lock().unwrap_or_else(|e| e.into_inner()) = if account_mismatch {
                            "Desktop Discord account differs from linked account".into()
                        } else {
                            "Waiting for desktop Discord (activity sharing must be enabled)".into()
                        };
                    }
                }
            }
        }
        let _ = wake.recv_timeout(Duration::from_millis(250));
    }
}

/// Reuses the maintained activity/protocol crate, adding bounded reads and transport deadlines.
struct Client {
    id: String,
    transport: Option<Transport>,
    user_id: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum NameConfirmation {
    Exact,
    Ignored,
    Unknown,
}

impl Client {
    fn publish_activity(&mut self, presence: &Presence) -> Result<NameConfirmation, Error> {
        static NEXT_NONCE: AtomicU64 = AtomicU64::new(1);
        let nonce = format!(
            "console-link-{}-{}",
            std::process::id(),
            NEXT_NONCE.fetch_add(1, Ordering::Relaxed)
        );
        let activity =
            serde_json::to_value(game_activity(presence)).map_err(|_| Error::JsonParseResponse)?;
        self.send(json!({ "cmd": "SET_ACTIVITY", "args": { "pid": std::process::id(), "activity": activity }, "nonce": nonce }), 1)?;
        let deadline = Instant::now() + config::IO_TIMEOUT;
        // Heartbeats may precede the command reply. A finite count/deadline prevents
        // an unrelated event or a stream of PINGs from being mistaken for acceptance.
        for _ in 0..4 {
            if Instant::now() >= deadline {
                return Err(Error::IPCConnectionFailed);
            }
            let (opcode, response) = self.recv()?;
            if opcode == 3 {
                self.send(response, 4)?;
                continue;
            }
            if opcode != 1
                || response.get("cmd").and_then(Value::as_str) != Some("SET_ACTIVITY")
                || response.get("nonce").and_then(Value::as_str) != Some(nonce.as_str())
                || !response.get("evt").is_none_or(Value::is_null)
            {
                return Err(Error::IPCConnectionFailed);
            }
            return Ok(
                match response.pointer("/data/name").and_then(Value::as_str) {
                    Some(name) if name == presence.game => NameConfirmation::Exact,
                    Some(_) => NameConfirmation::Ignored,
                    None => NameConfirmation::Unknown,
                },
            );
        }
        Err(Error::IPCConnectionFailed)
    }
}

impl DiscordIpc for Client {
    fn get_client_id(&self) -> &str {
        &self.id
    }
    fn connect_ipc(&mut self) -> Result<(), Error> {
        self.transport = Some(Transport::connect().map_err(|_| Error::IPCConnectionFailed)?);
        Ok(())
    }
    fn write(&mut self, data: &[u8]) -> Result<(), Error> {
        self.transport
            .as_mut()
            .ok_or(Error::NotConnected)?
            .write_all(data)
            .map_err(Error::WriteError)
    }
    fn read(&mut self, data: &mut [u8]) -> Result<(), Error> {
        self.transport
            .as_mut()
            .ok_or(Error::NotConnected)?
            .read_exact(data)
            .map_err(Error::ReadError)
    }
    fn close(&mut self) -> Result<(), Error> {
        self.transport = None;
        Ok(())
    }
    fn recv(&mut self) -> Result<(u32, Value), Error> {
        let mut header = [0; 8];
        self.read(&mut header)?;
        let (opcode, length) = decode_header(header).map_err(Error::ReadError)?;
        let mut bytes = vec![0; length];
        self.read(&mut bytes)?;
        let value = serde_json::from_slice(&bytes).map_err(|_| Error::JsonParseResponse)?;
        Ok((opcode, value))
    }
    fn send_handshake(&mut self) -> Result<(), Error> {
        self.send(json!({"v": 1, "client_id": self.id}), 0)?;
        let (opcode, response) = self.recv()?;
        if opcode != 1 || response.get("evt").and_then(Value::as_str) != Some("READY") {
            return Err(Error::IPCConnectionFailed);
        }
        self.user_id = response
            .pointer("/data/user/id")
            .and_then(Value::as_str)
            .filter(|value| valid_id(value))
            .map(str::to_owned);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn publish_requires_matching_ack_and_reports_name_confirmation() {
        fn receive(peer: &mut std::os::unix::net::UnixStream) -> (u32, Value) {
            let mut header = [0; 8];
            peer.read_exact(&mut header).unwrap();
            let (op, len) = decode_header(header).unwrap();
            let mut body = vec![0; len];
            peer.read_exact(&mut body).unwrap();
            (op, serde_json::from_slice(&body).unwrap())
        }
        fn send(peer: &mut std::os::unix::net::UnixStream, op: u32, body: Value) {
            let body = serde_json::to_vec(&body).unwrap();
            peer.write_all(&op.to_le_bytes()).unwrap();
            peer.write_all(&(body.len() as u32).to_le_bytes()).unwrap();
            peer.write_all(&body).unwrap();
        }
        for case in [
            "exact",
            "ignored",
            "unknown",
            "error",
            "nonce",
            "command",
            "opcode",
            "ping",
            "ping_flood",
        ] {
            let (socket, mut peer) = std::os::unix::net::UnixStream::pair().unwrap();
            socket.set_read_timeout(Some(config::IO_TIMEOUT)).unwrap();
            socket.set_write_timeout(Some(config::IO_TIMEOUT)).unwrap();
            peer.set_read_timeout(Some(config::IO_TIMEOUT)).unwrap();
            let server = std::thread::spawn(move || {
                let (op, request) = receive(&mut peer);
                assert_eq!(op, 1);
                assert_eq!(request["cmd"], "SET_ACTIVITY");
                assert_eq!(request["args"]["activity"]["name"], "Zelda");
                assert!(
                    request["nonce"]
                        .as_str()
                        .unwrap()
                        .starts_with("console-link-")
                );
                if case == "ping" || case == "ping_flood" {
                    for _ in 0..if case == "ping_flood" { 4 } else { 1 } {
                        send(&mut peer, 3, json!({"heartbeat": true}));
                        let (op, pong) = receive(&mut peer);
                        assert_eq!(op, 4);
                        assert_eq!(pong, json!({"heartbeat": true}));
                    }
                    if case == "ping_flood" {
                        return;
                    }
                }
                let mut reply = json!({"cmd": "SET_ACTIVITY", "nonce": request["nonce"], "evt": null, "data": {"name": "Zelda"}});
                match case {
                    "ignored" => reply["data"]["name"] = json!("Console Link"),
                    "unknown" => reply["data"] = Value::Null,
                    "error" => reply["evt"] = json!("ERROR"),
                    "nonce" => reply["nonce"] = json!("unrelated"),
                    "command" => reply["cmd"] = json!("CLEAR_ACTIVITY"),
                    _ => {}
                }
                send(&mut peer, if case == "opcode" { 2 } else { 1 }, reply);
            });
            let presence = Presence {
                application_id: "123456789012345678".into(),
                game: "Zelda".into(),
                started_at: 0,
                cover_url: None,
                game_url: None,
            };
            let mut client = Client {
                id: presence.application_id.clone(),
                transport: Some(Transport(socket)),
                user_id: None,
            };
            let result = client.publish_activity(&presence);
            match case {
                "exact" | "ping" => assert_eq!(result.unwrap(), NameConfirmation::Exact),
                "ignored" => assert_eq!(result.unwrap(), NameConfirmation::Ignored),
                "unknown" => assert_eq!(result.unwrap(), NameConfirmation::Unknown),
                _ => assert!(result.is_err(), "{case} must not count as accepted"),
            }
            server.join().unwrap();
        }
    }

    #[test]
    fn game_payload_has_specific_name_cover_link_and_original_timer() {
        let mut presence = Presence {
            application_id: "123456789012345678".into(),
            game: "Zelda".into(),
            started_at: 123,
            cover_url: Some("https://example.com/zelda.png".into()),
            game_url: Some("https://example.com/zelda".into()),
        };
        let payload = serde_json::to_value(game_activity(&presence)).unwrap();
        assert_eq!(payload["name"], "Zelda");
        assert_eq!(payload["details"], "Zelda");
        assert_eq!(payload["state"], "Nintendo Switch · Console Link");
        assert_eq!(payload["type"], 0);
        assert_eq!(payload["status_display_type"], 2);
        assert_eq!(payload["timestamps"]["start"], 123);
        assert_eq!(
            payload["assets"]["large_image"],
            "https://example.com/zelda.png"
        );
        assert_eq!(payload["buttons"][0]["url"], "https://example.com/zelda");
        presence.cover_url = None;
        presence.game_url = None;
        let minimal = serde_json::to_value(game_activity(&presence)).unwrap();
        assert!(minimal["assets"].get("large_image").is_none());
        assert!(minimal.get("buttons").is_none());
        assert_eq!(minimal["name"], "Zelda");
        presence.game_url = Some("javascript:alert(1)".into());
        assert!(!presence.valid());
        presence.game_url = Some("https://token@example.com".into());
        assert!(!presence.valid());
    }

    #[test]
    fn rejects_unbounded_packet_before_allocation() {
        let mut header = [0; 8];
        header[4..].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_header(header).is_err());
    }
    #[test]
    fn validates_id_and_manual_game() {
        let mut presence = Presence {
            application_id: "123456789012345678".into(),
            game: "Zelda".into(),
            started_at: 0,
            cover_url: None,
            game_url: None,
        };
        assert!(presence.valid());
        presence.application_id = "token".into();
        assert!(!presence.valid());
    }

    #[cfg(unix)]
    #[test]
    fn actual_transport_rejects_oversized_response_without_reading_body() {
        let (socket, mut peer) = std::os::unix::net::UnixStream::pair().unwrap();
        let mut header = [0; 8];
        header[4..].copy_from_slice(&u32::MAX.to_le_bytes());
        peer.write_all(&header).unwrap();
        let mut client = Client {
            id: String::new(),
            transport: Some(Transport(socket)),
            user_id: None,
        };
        assert!(client.recv().is_err());
    }

    #[test]
    fn linked_account_gate_is_fail_closed_for_missing_or_different_desktop_identity() {
        let linked = AccountRequirement::User("123456789012345678".into());
        assert!(account_allowed(&linked, Some("123456789012345678")));
        assert!(!account_allowed(&linked, Some("987654321098765432")));
        assert!(!account_allowed(&linked, None));
        assert!(!account_allowed(
            &AccountRequirement::SignedOut,
            Some("123456789012345678")
        ));
        assert!(account_allowed(&AccountRequirement::Legacy, None));
    }

    #[cfg(unix)]
    #[test]
    fn ready_handshake_records_desktop_identity_before_presence() {
        let (socket, mut peer) = std::os::unix::net::UnixStream::pair().unwrap();
        let ready = br#"{"evt":"READY","data":{"user":{"id":"123456789012345678"}}}"#;
        peer.write_all(&1_u32.to_le_bytes()).unwrap();
        peer.write_all(&(ready.len() as u32).to_le_bytes()).unwrap();
        peer.write_all(ready).unwrap();
        let mut client = Client {
            id: "1569416573180051537".into(),
            transport: Some(Transport(socket)),
            user_id: None,
        };
        client.send_handshake().unwrap();
        assert_eq!(client.user_id.as_deref(), Some("123456789012345678"));
        assert!(!account_allowed(
            &AccountRequirement::User("987654321098765432".into()),
            client.user_id.as_deref()
        ));
    }

    #[cfg(windows)]
    #[test]
    fn stalled_pipe_reader_cannot_block_write_forever() {
        use std::os::windows::io::AsRawHandle;
        use windows::{
            Win32::{
                Foundation::{CloseHandle, HANDLE},
                Storage::FileSystem::PIPE_ACCESS_DUPLEX,
                System::Pipes::{
                    CreateNamedPipeW, PIPE_NOWAIT, PIPE_TYPE_BYTE, PIPE_WAIT,
                    SetNamedPipeHandleState,
                },
            },
            core::PCWSTR,
        };
        let name = format!(
            r"\\.\pipe\console-link-test-{}-{}",
            std::process::id(),
            crate::capture::monotonic_ns()
        );
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        // SAFETY: name is NUL terminated; the test owns and closes the returned handle.
        let server = unsafe {
            CreateNamedPipeW(
                PCWSTR(wide.as_ptr()),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_WAIT,
                1,
                4096,
                4096,
                0,
                None,
            )
        };
        assert!(!server.is_invalid());
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(name)
            .unwrap();
        // SAFETY: same non-wait configuration as production; this test owns the client handle.
        unsafe {
            SetNamedPipeHandleState(HANDLE(file.as_raw_handle()), Some(&PIPE_NOWAIT), None, None)
                .unwrap();
        }
        let mut transport = Transport(file);
        let started = Instant::now();
        let result = transport.write_all(&vec![0; 1024 * 1024]);
        // SAFETY: this is the one owning test reference; client has a separate handle.
        unsafe {
            CloseHandle(server).unwrap();
        }
        assert!(result.is_err());
        assert!(started.elapsed() < Duration::from_secs(3));
    }
}
