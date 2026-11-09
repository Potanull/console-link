use crossbeam_channel::{Receiver, Sender, bounded};
use discord_rich_presence::{DiscordIpc, activity, error::Error};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const MAX_PACKET: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Presence {
    pub application_id: String,
    pub game: String,
    pub started_at: i64,
}

impl Presence {
    pub fn valid(&self) -> bool {
        (16..=22).contains(&self.application_id.len())
            && self.application_id.bytes().all(|b| b.is_ascii_digit())
            && !self.game.trim().is_empty()
            && self.game.chars().count() <= 128
    }
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
            .filter(|_| wanted != sent || last_try.elapsed() >= Duration::from_secs(15))
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
            if last_try.elapsed() >= Duration::from_secs(5) {
                last_try = Instant::now();
                let mut account_mismatch = false;
                let result = (|| -> Result<(), Error> {
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
                    connected.set_activity(
                        activity::Activity::new()
                            .details(presence.game.as_str())
                            .state("Playing on console")
                            .timestamps(activity::Timestamps::new().start(presence.started_at)),
                    )?;
                    let (opcode, response) = connected.recv()?;
                    if opcode != 1 || response.get("evt").and_then(Value::as_str) == Some("ERROR") {
                        return Err(Error::IPCConnectionFailed);
                    }
                    Ok(())
                })();
                match result {
                    Ok(()) => {
                        sent = wanted;
                        *status.lock().unwrap_or_else(|e| e.into_inner()) =
                            "Activity sent to desktop Discord".into();
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
            .filter(|value| {
                (16..=22).contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_digit())
            })
            .map(str::to_owned);
        Ok(())
    }
}

fn decode_header(header: [u8; 8]) -> std::io::Result<(u32, usize)> {
    let opcode = u32::from_le_bytes(header[..4].try_into().expect("four bytes"));
    let length = u32::from_le_bytes(header[4..].try_into().expect("four bytes")) as usize;
    if length > MAX_PACKET {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Discord packet exceeds limit",
        ));
    }
    Ok((opcode, length))
}

#[cfg(unix)]
struct Transport(std::os::unix::net::UnixStream);

#[cfg(unix)]
impl Transport {
    fn connect() -> std::io::Result<Self> {
        let mut directories = vec![std::path::PathBuf::from("/tmp")];
        for key in ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"] {
            if let Some(value) = std::env::var_os(key) {
                directories.push(value.into());
            }
        }
        for directory in directories {
            for n in 0..10 {
                if let Ok(stream) = std::os::unix::net::UnixStream::connect(
                    directory.join(format!("discord-ipc-{n}")),
                ) {
                    stream.set_read_timeout(Some(Duration::from_millis(500)))?;
                    stream.set_write_timeout(Some(Duration::from_millis(500)))?;
                    return Ok(Self(stream));
                }
            }
        }
        Err(std::io::ErrorKind::NotFound.into())
    }
    fn read_exact(&mut self, bytes: &mut [u8]) -> std::io::Result<()> {
        self.0.read_exact(bytes)
    }
    fn write_all(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.0.write_all(bytes)
    }
}

#[cfg(windows)]
struct Transport(std::fs::File);

#[cfg(windows)]
impl Transport {
    fn connect() -> std::io::Result<Self> {
        use std::os::windows::fs::OpenOptionsExt;
        for n in 0..10 {
            if let Ok(file) = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(0x0010_0000 | 0x0001_0000)
                .open(format!(r"\\.\pipe\discord-ipc-{n}"))
            {
                use std::os::windows::io::AsRawHandle;
                use windows::Win32::{
                    Foundation::HANDLE,
                    System::Pipes::{PIPE_NOWAIT, SetNamedPipeHandleState},
                };
                // SAFETY: this process owns the client pipe. Non-wait mode bounds both reads and writes.
                unsafe {
                    SetNamedPipeHandleState(
                        HANDLE(file.as_raw_handle()),
                        Some(&PIPE_NOWAIT),
                        None,
                        None,
                    )
                }
                .map_err(std::io::Error::other)?;
                return Ok(Self(file));
            }
        }
        Err(std::io::ErrorKind::NotFound.into())
    }
    fn read_exact(&mut self, mut bytes: &mut [u8]) -> std::io::Result<()> {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::{Foundation::HANDLE, System::Pipes::PeekNamedPipe};
        let deadline = Instant::now() + Duration::from_millis(500);
        while !bytes.is_empty() {
            let mut available = 0;
            // SAFETY: file owns a live pipe handle; only the output count is requested.
            unsafe {
                PeekNamedPipe(
                    HANDLE(self.0.as_raw_handle()),
                    None,
                    0,
                    None,
                    Some(&mut available),
                    None,
                )
            }
            .map_err(std::io::Error::other)?;
            if available == 0 {
                if Instant::now() >= deadline {
                    return Err(std::io::ErrorKind::TimedOut.into());
                }
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
            let amount = bytes.len().min(available as usize);
            self.0.read_exact(&mut bytes[..amount])?;
            bytes = &mut bytes[amount..];
        }
        Ok(())
    }
    fn write_all(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        let deadline = Instant::now() + Duration::from_millis(500);
        let mut remaining = bytes;
        while !remaining.is_empty() {
            match self.0.write(remaining) {
                Ok(0) => {}
                Ok(written) => {
                    remaining = &remaining[written..];
                    continue;
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        || error.raw_os_error() == Some(232) => {}
                Err(error) => return Err(error),
            }
            if Instant::now() >= deadline {
                return Err(std::io::ErrorKind::TimedOut.into());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            crate::types::monotonic_ns()
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
