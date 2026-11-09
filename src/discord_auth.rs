//! Discord account identity via public-client PKCE. Activity remains desktop IPC.
//! Tokens never cross this module's public API and are persisted only in the OS credential vault.

use crossbeam_channel::{Sender, bounded};
use oauth2::{
    AuthType, AuthUrl, AuthorizationCode, ClientId, CsrfToken, EndpointNotSet, EndpointSet,
    PkceCodeChallenge, RedirectUrl, RefreshToken, Scope, SyncHttpClient, TokenResponse, TokenUrl,
    basic::{BasicClient, BasicTokenResponse},
    reqwest,
};
use serde::Deserialize;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
use url::Url;

pub const DEFAULT_APPLICATION_ID: &str = "1569416573180051537";
pub const REDIRECT_URI: &str = "http://127.0.0.1:45961/discord/callback";
const CALLBACK_ADDRESS: &str = "127.0.0.1:45961";
const AUTHORIZE_URL: &str = "https://discord.com/oauth2/authorize";
const TOKEN_URL: &str = "https://discord.com/api/oauth2/token";
const PROFILE_URL: &str = "https://discord.com/api/v10/users/@me";
const REVOKE_URL: &str = "https://discord.com/api/oauth2/token/revoke";
const MAX_HEADER_BYTES: usize = 8192;
const MAX_RESPONSE_BYTES: u64 = 64 * 1024;
const LOGIN_TIMEOUT: Duration = Duration::from_secs(180);

type PublicClient =
    BasicClient<EndpointSet, EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointSet>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscordAccount {
    pub id: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthStatus {
    Disconnected,
    Connecting,
    Connected(DiscordAccount),
    Error(String),
}

#[derive(Clone, Copy)]
enum Command {
    Connect,
    Disconnect,
}

struct Shared {
    status: Mutex<AuthStatus>,
    intent: Mutex<Option<(u64, Command)>>,
    generation: AtomicU64,
    completion: AtomicU64,
    stopped: AtomicBool,
}

impl Shared {
    fn current(&self, generation: u64) -> bool {
        !self.stopped.load(Ordering::Acquire)
            && self.generation.load(Ordering::Acquire) == generation
    }
    fn set_status(&self, generation: u64, status: AuthStatus) {
        let mut current = self.status.lock().unwrap_or_else(|e| e.into_inner());
        if self.current(generation) {
            *current = status;
        }
    }
}

/// One cancellable owner thread handles OAuth/network/keychain operations away from the UI.
pub struct DiscordAuth {
    application_id: String,
    shared: Arc<Shared>,
    wake: Sender<()>,
}

impl DiscordAuth {
    pub fn new(application_id: String) -> Self {
        let shared = Arc::new(Shared {
            status: Mutex::new(AuthStatus::Disconnected),
            intent: Mutex::new(None),
            generation: AtomicU64::new(0),
            completion: AtomicU64::new(0),
            stopped: AtomicBool::new(false),
        });
        let (wake, receiver) = bounded(1);
        let worker_shared = Arc::clone(&shared);
        let worker_id = application_id.clone();
        std::thread::spawn(move || {
            if !valid_id(&worker_id) {
                worker_shared.set_status(
                    0,
                    AuthStatus::Error("В этой сборке не настроен Discord Application ID.".into()),
                );
                return;
            }
            let Ok(http) = BoundedHttp::new() else {
                worker_shared.set_status(
                    0,
                    AuthStatus::Error(
                        "Не удалось подготовить безопасное соединение Discord.".into(),
                    ),
                );
                return;
            };
            let vault = NativeVault {
                application_id: worker_id.clone(),
            };
            let mut session = restore_session(&worker_id, &http, &vault, &worker_shared);
            let mut processed = 0;
            while !worker_shared.stopped.load(Ordering::Acquire) {
                let intent = *worker_shared
                    .intent
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                if let Some((generation, command)) =
                    intent.filter(|(generation, _)| *generation != processed)
                {
                    processed = generation;
                    match command {
                        Command::Connect => {
                            session = None;
                            match login(&worker_id, &http, &vault, &worker_shared, generation) {
                                Ok(connected) => {
                                    if worker_shared.current(generation) {
                                        worker_shared.completion.fetch_add(1, Ordering::AcqRel);
                                    }
                                    worker_shared.set_status(
                                        generation,
                                        AuthStatus::Connected(
                                            connected.account.clone().expect("verified profile"),
                                        ),
                                    );
                                    session = Some(connected);
                                }
                                Err(Failure::Retained { refresh, message }) => {
                                    session = Some(Session::retry(refresh, None));
                                    worker_shared
                                        .set_status(generation, AuthStatus::Error(message.into()));
                                }
                                Err(Failure::Cancelled) => {}
                                Err(Failure::Expired) => worker_shared.set_status(
                                    generation,
                                    AuthStatus::Error(
                                        "Вход Discord отозван. Подключись повторно.".into(),
                                    ),
                                ),
                                Err(Failure::Message(error)) => worker_shared
                                    .set_status(generation, AuthStatus::Error(error.into())),
                            }
                        }
                        Command::Disconnect => {
                            let previous = session.take();
                            let cleared = vault.clear();
                            if let Some(previous) = previous {
                                revoke_best_effort(&worker_id, &http, &previous.refresh);
                            }
                            worker_shared.set_status(generation, if cleared.is_ok() { AuthStatus::Disconnected } else { AuthStatus::Error("Не удалось удалить сохраненный вход. Разреши доступ к хранилищу учетных данных и повтори отключение.".into()) });
                        }
                    }
                } else if let Some(previous) = session
                    .as_ref()
                    .filter(|session| Instant::now() >= session.refresh_at)
                {
                    let generation = worker_shared.generation.load(Ordering::Acquire);
                    match refresh_session(
                        &worker_id,
                        &http,
                        &vault,
                        &previous.refresh,
                        previous.account.as_ref().map(|account| account.id.as_str()),
                        &worker_shared,
                        generation,
                    ) {
                        Ok(refreshed) => {
                            worker_shared.set_status(
                                generation,
                                AuthStatus::Connected(
                                    refreshed.account.clone().expect("verified profile"),
                                ),
                            );
                            session = Some(refreshed);
                        }
                        Err(Failure::Retained { refresh, message }) => {
                            let account =
                                session.as_ref().and_then(|session| session.account.clone());
                            session = Some(Session::retry(refresh, account));
                            worker_shared.set_status(generation, AuthStatus::Error(message.into()));
                        }
                        Err(Failure::Cancelled) => {
                            session = None;
                        }
                        Err(Failure::Expired) => {
                            session = None;
                            let _ = vault.clear();
                            worker_shared.set_status(
                                generation,
                                AuthStatus::Error(
                                    "Вход Discord отозван. Подключись повторно.".into(),
                                ),
                            );
                        }
                        Err(Failure::Message(error)) => {
                            session.as_mut().expect("refresh session exists").refresh_at =
                                Instant::now() + Duration::from_secs(30);
                            worker_shared.set_status(generation, AuthStatus::Error(error.into()));
                        }
                    }
                }
                let _ = receiver.recv_timeout(Duration::from_millis(200));
            }
        });
        Self {
            application_id,
            shared,
            wake,
        }
    }

    /// Increments only after a successful explicit login; URI wakeups never authorize an account.
    pub fn completion_generation(&self) -> u64 {
        self.shared.completion.load(Ordering::Acquire)
    }

    pub fn application_id(&self) -> &str {
        &self.application_id
    }
    pub fn connect(&self) {
        self.command(Command::Connect, AuthStatus::Connecting);
    }
    pub fn disconnect(&self) {
        self.command(Command::Disconnect, AuthStatus::Disconnected);
    }
    fn command(&self, command: Command, status: AuthStatus) {
        let mut intent = self.shared.intent.lock().unwrap_or_else(|e| e.into_inner());
        let generation = self.shared.generation.fetch_add(1, Ordering::AcqRel) + 1;
        *self.shared.status.lock().unwrap_or_else(|e| e.into_inner()) = status;
        *intent = Some((generation, command));
        let _ = self.wake.try_send(());
    }
    pub fn status(&self) -> AuthStatus {
        self.shared
            .status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    pub fn account(&self) -> Option<DiscordAccount> {
        match self.status() {
            AuthStatus::Connected(account) => Some(account),
            _ => None,
        }
    }
}

impl Drop for DiscordAuth {
    fn drop(&mut self) {
        self.shared.stopped.store(true, Ordering::Release);
        self.shared.generation.fetch_add(1, Ordering::AcqRel);
        let _ = self.wake.try_send(());
    }
}

struct Session {
    account: Option<DiscordAccount>,
    refresh: RefreshToken,
    refresh_at: Instant,
}
impl Session {
    fn retry(refresh: RefreshToken, account: Option<DiscordAccount>) -> Self {
        Self {
            account,
            refresh,
            refresh_at: Instant::now() + Duration::from_secs(30),
        }
    }
}

enum Failure {
    Retained {
        refresh: RefreshToken,
        message: &'static str,
    },
    Cancelled,
    Expired,
    Message(&'static str),
}

fn client(application_id: &str) -> PublicClient {
    BasicClient::new(ClientId::new(application_id.into()))
        .set_auth_type(AuthType::RequestBody)
        .set_auth_uri(AuthUrl::new(AUTHORIZE_URL.into()).expect("constant authorization URL"))
        .set_token_uri(TokenUrl::new(TOKEN_URL.into()).expect("constant token URL"))
        .set_redirect_uri(RedirectUrl::new(REDIRECT_URI.into()).expect("constant loopback URL"))
}

fn login(
    application_id: &str,
    http: &BoundedHttp,
    vault: &impl Vault,
    shared: &Shared,
    generation: u64,
) -> Result<Session, Failure> {
    let listener = TcpListener::bind(CALLBACK_ADDRESS).map_err(|_| {
        Failure::Message(
            "Порт входа Discord занят. Закрой другое окно Console Link и повтори вход.",
        )
    })?;
    listener
        .set_nonblocking(true)
        .map_err(|_| Failure::Message("Не удалось открыть локальный приемник входа."))?;
    let client = client(application_id);
    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let (authorize_url, state) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("identify".into()))
        .set_pkce_challenge(challenge)
        .url();
    if !shared.current(generation) {
        return Err(Failure::Cancelled);
    }
    webbrowser::open(authorize_url.as_str())
        .map_err(|_| Failure::Message("Не удалось открыть браузер для входа Discord."))?;
    let code = wait_callback(&listener, state.secret(), shared, generation)?;
    if !shared.current(generation) {
        return Err(Failure::Cancelled);
    }
    let token = client.exchange_code(code).set_pkce_verifier(verifier).request(http).map_err(|_| Failure::Message("Discord отклонил вход. Проверь Public Client и разрешенный адрес перенаправления приложения."))?;
    session_from_token(token, None, http, vault, shared, generation)
}

fn restore_session(
    application_id: &str,
    http: &BoundedHttp,
    vault: &impl Vault,
    shared: &Shared,
) -> Option<Session> {
    let refresh = match vault.load() {
        Ok(Some(refresh)) => RefreshToken::new(refresh),
        Ok(None) => return None,
        Err(_) => {
            shared.set_status(0, AuthStatus::Error("Не удалось прочитать сохраненный вход Discord. Разреши доступ к хранилищу учетных данных.".into()));
            return None;
        }
    };
    shared.set_status(0, AuthStatus::Connecting);
    restored_session(
        refresh_session(application_id, http, vault, &refresh, None, shared, 0),
        refresh,
        vault,
        shared,
    )
}

fn restored_session(
    result: Result<Session, Failure>,
    refresh: RefreshToken,
    vault: &impl Vault,
    shared: &Shared,
) -> Option<Session> {
    match result {
        Ok(session) => {
            shared.set_status(
                0,
                AuthStatus::Connected(session.account.clone().expect("verified profile")),
            );
            Some(session)
        }
        Err(Failure::Retained { refresh, message }) => {
            shared.set_status(0, AuthStatus::Error(message.into()));
            Some(Session::retry(refresh, None))
        }
        Err(Failure::Cancelled) => None,
        Err(Failure::Expired) => {
            let _ = vault.clear();
            shared.set_status(
                0,
                AuthStatus::Error("Вход Discord отозван. Подключись повторно.".into()),
            );
            None
        }
        Err(Failure::Message(error)) => {
            shared.set_status(0, AuthStatus::Error(error.into()));
            Some(Session::retry(refresh, None))
        }
    }
}

fn refresh_session(
    application_id: &str,
    http: &BoundedHttp,
    vault: &impl Vault,
    refresh: &RefreshToken,
    expected_id: Option<&str>,
    shared: &Shared,
    generation: u64,
) -> Result<Session, Failure> {
    if !shared.current(generation) {
        return Err(Failure::Cancelled);
    }
    let mut token = client(application_id).exchange_refresh_token(refresh).request(http).map_err(|error| {
        if matches!(error, oauth2::RequestTokenError::ServerResponse(ref response) if response.error() == &oauth2::basic::BasicErrorResponseType::InvalidGrant) { Failure::Expired }
        else { Failure::Message("Discord временно недоступен. Вход будет проверен повторно; сохраненные данные не удалены.") }
    })?;
    if token.refresh_token().is_none() {
        token.set_refresh_token(Some(refresh.clone()));
    }
    let session = session_from_token(token, expected_id, http, vault, shared, generation)?;
    Ok(session)
}

fn session_from_token(
    token: BasicTokenResponse,
    expected_id: Option<&str>,
    http: &BoundedHttp,
    vault: &impl Vault,
    shared: &Shared,
    generation: u64,
) -> Result<Session, Failure> {
    finish_token(token, expected_id, vault, shared, generation, |token| {
        let response = http
            .inner
            .get(PROFILE_URL)
            .bearer_auth(token.access_token().secret())
            .send()
            .map_err(|_| {
                Failure::Message(
                    "Не удалось проверить аккаунт Discord. Вход будет проверен повторно.",
                )
            })?;
        if !response.status().is_success() {
            return Err(Failure::Message(
                "Discord не подтвердил аккаунт. Вход будет проверен повторно.",
            ));
        }
        parse_account(
            &read_bounded(response)
                .map_err(|_| Failure::Message("Некорректный ответ профиля Discord."))?,
        )
        .map_err(|_| Failure::Message("Некорректный профиль Discord."))
    })
}

fn finish_token(
    token: BasicTokenResponse,
    expected_id: Option<&str>,
    vault: &impl Vault,
    shared: &Shared,
    generation: u64,
    profile: impl FnOnce(&BasicTokenResponse) -> Result<DiscordAccount, Failure>,
) -> Result<Session, Failure> {
    if !shared.current(generation) {
        return Err(Failure::Cancelled);
    }
    let refresh = token.refresh_token().cloned().ok_or(Failure::Message(
        "Discord не предоставил долговременный вход. Проверь настройки Public Client.",
    ))?;
    // A successful exchange can invalidate the old refresh token immediately.
    // Persist its replacement before profile I/O; retain it in memory on any later failure.
    let result = (|| {
        vault.save(refresh.secret()).map_err(|_| Failure::Message("Не удалось сохранить вход безопасно. Разреши доступ к Keychain или диспетчеру учетных данных Windows."))?;
        if !shared.current(generation) {
            return Err(Failure::Cancelled);
        }
        let account = profile(&token)?;
        if expected_id.is_some_and(|expected| expected != account.id) {
            return Err(Failure::Message(
                "Аккаунт Discord изменился. Подключись повторно.",
            ));
        }
        Ok(account)
    })();
    if !shared.current(generation) {
        let _ = vault.clear();
        return Err(Failure::Cancelled);
    }
    let account = match result {
        Ok(account) => account,
        Err(Failure::Message(message)) => return Err(Failure::Retained { refresh, message }),
        Err(error) => return Err(error),
    };
    let lifetime = token.expires_in().unwrap_or(Duration::from_secs(3600));
    let refresh_after = lifetime
        .saturating_sub(Duration::from_secs(60))
        .clamp(Duration::from_secs(30), Duration::from_secs(3600));
    Ok(Session {
        account: Some(account),
        refresh,
        refresh_at: Instant::now() + refresh_after,
    })
}

fn revoke_best_effort(application_id: &str, http: &BoundedHttp, refresh: &RefreshToken) {
    let _ = http
        .inner
        .post(REVOKE_URL)
        .form(&[
            ("client_id", application_id),
            ("token", refresh.secret().as_str()),
            ("token_type_hint", "refresh_token"),
        ])
        .send();
}

trait Vault {
    fn load(&self) -> Result<Option<String>, ()>;
    fn save(&self, refresh: &str) -> Result<(), ()>;
    fn clear(&self) -> Result<(), ()>;
}
struct NativeVault {
    application_id: String,
}
impl NativeVault {
    fn entry(&self) -> Result<keyring::Entry, ()> {
        keyring::Entry::new(
            "io.github.potanull.consolelink.discord",
            &self.application_id,
        )
        .map_err(|_| ())
    }
}
impl Vault for NativeVault {
    fn load(&self) -> Result<Option<String>, ()> {
        match self.entry()?.get_password() {
            Ok(value) if value.len() <= 8192 => Ok(Some(value)),
            Ok(_) => Err(()),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(()),
        }
    }
    fn save(&self, refresh: &str) -> Result<(), ()> {
        self.entry()?.set_password(refresh).map_err(|_| ())
    }
    fn clear(&self) -> Result<(), ()> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(()),
        }
    }
}

struct BoundedHttp {
    inner: reqwest::blocking::Client,
}
impl BoundedHttp {
    fn new() -> Result<Self, reqwest::Error> {
        Ok(Self {
            inner: reqwest::blocking::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(4))
                .timeout(Duration::from_secs(8))
                .user_agent("Console-Link/Discord-OAuth")
                .build()?,
        })
    }
}
impl SyncHttpClient for BoundedHttp {
    type Error = std::io::Error;
    fn call(&self, request: oauth2::HttpRequest) -> Result<oauth2::HttpResponse, Self::Error> {
        let request: reqwest::blocking::Request = request
            .try_into()
            .map_err(|_| std::io::Error::other("Invalid OAuth request"))?;
        let response = self
            .inner
            .execute(request)
            .map_err(|_| std::io::Error::other("OAuth network request failed"))?;
        let mut builder = oauth2::http::Response::builder()
            .status(response.status())
            .version(response.version());
        for (name, value) in response.headers() {
            builder = builder.header(name, value);
        }
        builder
            .body(read_bounded(response)?)
            .map_err(|_| std::io::Error::other("Invalid OAuth response"))
    }
}
fn read_bounded(response: reqwest::blocking::Response) -> std::io::Result<Vec<u8>> {
    let mut body = Vec::new();
    response
        .take(MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut body)?;
    if body.len() > MAX_RESPONSE_BYTES as usize {
        return Err(std::io::Error::other("OAuth response exceeds limit"));
    }
    Ok(body)
}

enum Callback {
    Code(AuthorizationCode),
    Denied,
}

fn wait_callback(
    listener: &TcpListener,
    expected_state: &str,
    shared: &Shared,
    generation: u64,
) -> Result<AuthorizationCode, Failure> {
    let deadline = Instant::now() + LOGIN_TIMEOUT;
    while Instant::now() < deadline {
        if !shared.current(generation) {
            return Err(Failure::Cancelled);
        }
        match listener.accept() {
            Ok((mut stream, peer)) if peer.ip().is_loopback() => {
                let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
                let _ = stream.set_write_timeout(Some(Duration::from_millis(200)));
                let result = read_header(&mut stream, shared, generation)
                    .and_then(|header| parse_callback(&header, expected_state));
                match result {
                    Ok(Callback::Code(code)) => {
                        reply(&mut stream, true);
                        return Ok(code);
                    }
                    Ok(Callback::Denied) => {
                        reply(&mut stream, false);
                        return Err(Failure::Message("Вход отменен в Discord."));
                    }
                    Err(()) => reply(&mut stream, false),
                }
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50))
            }
            Err(_) => return Err(Failure::Message("Локальный приемник входа недоступен.")),
        }
    }
    Err(Failure::Message(
        "Время ожидания Discord истекло. Нажми подключение повторно.",
    ))
}

fn read_header(stream: &mut TcpStream, shared: &Shared, generation: u64) -> Result<Vec<u8>, ()> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut header = Vec::new();
    let mut buffer = [0; 512];
    while header.len() < MAX_HEADER_BYTES && Instant::now() < deadline && shared.current(generation)
    {
        match stream.read(&mut buffer) {
            Ok(0) => return Err(()),
            Ok(count) => {
                header.extend_from_slice(&buffer[..count]);
                if header.windows(4).any(|w| w == b"\r\n\r\n") {
                    return Ok(header);
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return Err(()),
        }
    }
    Err(())
}

fn parse_callback(header: &[u8], expected_state: &str) -> Result<Callback, ()> {
    if header.len() > MAX_HEADER_BYTES {
        return Err(());
    }
    let text = std::str::from_utf8(header).map_err(|_| ())?;
    let (text, remainder) = text.split_once("\r\n\r\n").ok_or(())?;
    if !remainder.is_empty() {
        return Err(());
    }
    let mut lines = text.split("\r\n");
    let mut request = lines.next().ok_or(())?.split_whitespace();
    if request.next() != Some("GET") {
        return Err(());
    }
    let target = request.next().ok_or(())?;
    if !target.starts_with("/discord/callback?")
        || request.next() != Some("HTTP/1.1")
        || request.next().is_some()
    {
        return Err(());
    }
    let mut host_seen = false;
    for line in lines.take_while(|line| !line.is_empty()) {
        let (name, value) = line.split_once(':').ok_or(())?;
        if name.eq_ignore_ascii_case("host") {
            if host_seen || value.trim() != CALLBACK_ADDRESS {
                return Err(());
            }
            host_seen = true;
        }
        if name.eq_ignore_ascii_case("content-length") && value.trim() != "0" {
            return Err(());
        }
        if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err(());
        }
    }
    if !host_seen {
        return Err(());
    }
    let url = Url::parse(&format!("http://{CALLBACK_ADDRESS}{target}")).map_err(|_| ())?;
    if url.path() != "/discord/callback" || url.fragment().is_some() {
        return Err(());
    }
    let mut state = None;
    let mut code = None;
    let mut error = None;
    for (key, value) in url.query_pairs() {
        let slot = match key.as_ref() {
            "state" => &mut state,
            "code" => &mut code,
            "error" => &mut error,
            _ => continue,
        };
        if slot.replace(value.into_owned()).is_some() {
            return Err(());
        }
    }
    let state = CsrfToken::new(state.ok_or(())?);
    if state != CsrfToken::new(expected_state.to_owned()) {
        return Err(());
    }
    match (code, error) {
        (Some(code), None) if !code.is_empty() && code.len() <= 2048 => {
            Ok(Callback::Code(AuthorizationCode::new(code)))
        }
        (None, Some(_)) => Ok(Callback::Denied),
        _ => Err(()),
    }
}

fn reply(stream: &mut TcpStream, success: bool) {
    let _ = stream.write_all(completion_response(success).as_bytes());
}

fn completion_response(success: bool) -> String {
    let (status, script, script_hash, heading, message, button) = if success {
        (
            "200 OK",
            "history.replaceState(null,'','/discord/complete');setTimeout(()=>location.assign('console-link://auth/complete'),150);",
            "6MEkWWTxwHzvUzyMvAEk4UxJFw4DZKA0t1pb/0N7Vjk=",
            "Возвращаемся в Console Link",
            "Запрос Discord получен. Приложение завершает проверку аккаунта.",
            "<a href=\"console-link://auth/complete\">Открыть Console Link</a>",
        )
    } else {
        (
            "400 Bad Request",
            "history.replaceState(null,'','/discord/complete');",
            "5RDgjC2qQzkZJiiHDmWT0sZuAyMDlXM8o/nvGc5LNiw=",
            "Запрос не принят",
            "Вернись в Console Link, чтобы отменить или повторить вход.",
            "",
        )
    };
    // Static content only. No code, state, token or provider text enters the response/URI.
    let body = format!(
        r#"<!doctype html><html lang="ru"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Console Link</title><script>{script}</script><style>html{{color-scheme:dark;font:16px system-ui;background:#101114;color:#f5f5f7}}body{{min-height:90vh;display:grid;place-items:center;margin:0;padding:24px;box-sizing:border-box}}main{{max-width:440px;background:#1b1c20;border:1px solid #36373d;border-radius:24px;padding:40px}}.brand{{font-size:12px;letter-spacing:3px;color:#aaa}}h1{{font-size:28px;line-height:1.2}}p{{color:#aaa;line-height:1.6}}a{{display:inline-block;margin-top:16px;padding:14px 20px;border-radius:12px;background:#fff;color:#111;text-decoration:none;font-weight:600}}</style><main><div class="brand">CONSOLE LINK</div><h1>{heading}</h1><p>{message}</p>{button}<p>Если браузер запросит разрешение, подтверди открытие приложения. Затем эту вкладку можно закрыть.</p></main></html>"#
    );
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nCache-Control: no-store\r\nContent-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; script-src 'sha256-{script_hash}'\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
}

fn valid_id(id: &str) -> bool {
    (16..=22).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit())
}
fn parse_account(bytes: &[u8]) -> Result<DiscordAccount, ()> {
    #[derive(Deserialize)]
    struct Profile {
        id: String,
        username: String,
        global_name: Option<String>,
        avatar: Option<String>,
        discriminator: Option<String>,
    }
    let profile: Profile = serde_json::from_slice(bytes).map_err(|_| ())?;
    if !valid_id(&profile.id) {
        return Err(());
    }
    let name = profile
        .global_name
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(profile.username);
    let display_name: String = name
        .chars()
        .filter(|character| !character.is_control())
        .take(80)
        .collect();
    if display_name.trim().is_empty() {
        return Err(());
    }
    let avatar_url = avatar_url(
        &profile.id,
        profile.avatar.as_deref(),
        profile.discriminator.as_deref(),
    );
    Ok(DiscordAccount {
        id: profile.id,
        display_name,
        avatar_url,
    })
}

fn avatar_url(id: &str, hash: Option<&str>, discriminator: Option<&str>) -> Option<String> {
    let id_number = id.parse::<u64>().ok()?;
    if let Some(hash) = hash {
        let plain = hash.strip_prefix("a_").unwrap_or(hash);
        if plain.len() != 32 || !plain.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        return Some(format!(
            "https://cdn.discordapp.com/avatars/{id}/{hash}.png?size=128"
        ));
    }
    let legacy = discriminator
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|value| *value != 0);
    let index = legacy.map_or((id_number >> 22) % 6, |value| u64::from(value % 5));
    Some(format!(
        "https://cdn.discordapp.com/embed/avatars/{index}.png"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avatar_uses_validated_cdn_hash_and_default_avatar_without_extra_scopes() {
        let profile = parse_account(br#"{"id":"123456789012345678","username":"Player","avatar":"a_0123456789abcdef0123456789abcdef"}"#).unwrap();
        assert_eq!(
            profile.avatar_url.as_deref(),
            Some(
                "https://cdn.discordapp.com/avatars/123456789012345678/a_0123456789abcdef0123456789abcdef.png?size=128"
            )
        );
        for hash in [
            "../private",
            "https://attacker.example/image",
            "a_short",
            "0123456789abcdef0123456789abcdeg",
        ] {
            assert!(avatar_url("123456789012345678", Some(hash), None).is_none());
        }
        let default = avatar_url("123456789012345678", None, Some("0")).unwrap();
        assert_eq!(
            default,
            format!(
                "https://cdn.discordapp.com/embed/avatars/{}.png",
                (123456789012345678u64 >> 22) % 6
            )
        );
        assert_eq!(
            avatar_url("123456789012345678", None, Some("1234")).as_deref(),
            Some("https://cdn.discordapp.com/embed/avatars/4.png")
        );
    }

    #[test]
    fn only_valid_callback_can_redirect_to_tokenless_application_wakeup() {
        let valid = b"GET /discord/callback?code=private-code&state=expected HTTP/1.1\r\nHost: 127.0.0.1:45961\r\n\r\n";
        let accepted = matches!(parse_callback(valid, "expected"), Ok(Callback::Code(_)));
        let response = completion_response(accepted);
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(!response.contains("Location:"));
        assert!(response.contains("history.replaceState(null,'','/discord/complete');setTimeout(()=>location.assign('console-link://auth/complete'),150);"));
        assert!(!response.contains("private-code"));
        assert!(!response.contains("expected"));
        assert!(response.contains("href=\"console-link://auth/complete\""));
        let rejected = completion_response(matches!(
            parse_callback(valid, "wrong-state"),
            Ok(Callback::Code(_))
        ));
        assert!(rejected.starts_with("HTTP/1.1 400 Bad Request"));
        assert!(!rejected.contains("Location:"));
        assert!(!rejected.contains("console-link://"));
    }

    #[test]
    fn offline_restore_retries_saved_credential_and_only_invalid_grant_clears_it() {
        use std::cell::Cell;
        struct FakeVault {
            clears: Cell<usize>,
        }
        impl Vault for FakeVault {
            fn load(&self) -> Result<Option<String>, ()> {
                Ok(Some("saved-refresh".into()))
            }
            fn save(&self, _: &str) -> Result<(), ()> {
                Ok(())
            }
            fn clear(&self) -> Result<(), ()> {
                self.clears.set(self.clears.get() + 1);
                Ok(())
            }
        }
        struct RecoveredProvider;
        impl SyncHttpClient for RecoveredProvider {
            type Error = std::io::Error;
            fn call(
                &self,
                request: oauth2::HttpRequest,
            ) -> Result<oauth2::HttpResponse, Self::Error> {
                let fields: std::collections::HashMap<_, _> =
                    url::form_urlencoded::parse(request.body()).collect();
                assert_eq!(
                    fields.get("refresh_token").map(|s| s.as_ref()),
                    Some("saved-refresh")
                );
                Ok(oauth2::http::Response::builder().status(200).header("content-type", "application/json")
                    .body(br#"{"access_token":"access","token_type":"Bearer","refresh_token":"recovered-refresh","expires_in":3600}"#.to_vec()).unwrap())
            }
        }
        let shared = Shared {
            status: Mutex::new(AuthStatus::Disconnected),
            intent: Mutex::new(None),
            generation: AtomicU64::new(0),
            completion: AtomicU64::new(0),
            stopped: AtomicBool::new(false),
        };
        let vault = FakeVault {
            clears: Cell::new(0),
        };
        let pending = restored_session(
            Err(Failure::Message("temporary network failure")),
            RefreshToken::new(vault.load().unwrap().unwrap()),
            &vault,
            &shared,
        )
        .expect("offline startup must schedule a retry");
        assert!(pending.account.is_none());
        assert!(pending.refresh_at > Instant::now());
        assert_eq!(vault.clears.get(), 0);
        let token = client(DEFAULT_APPLICATION_ID)
            .exchange_refresh_token(&pending.refresh)
            .request(&RecoveredProvider)
            .unwrap();
        let recovered = finish_token(token, None, &vault, &shared, 0, |_| {
            Ok(DiscordAccount {
                id: "123456789012345678".into(),
                display_name: "Player".into(),
                avatar_url: None,
            })
        });
        let session = restored_session(recovered, pending.refresh, &vault, &shared).unwrap();
        assert!(session.account.is_some());
        assert_eq!(vault.clears.get(), 0);
        assert!(
            restored_session(Err(Failure::Expired), session.refresh, &vault, &shared).is_none()
        );
        assert_eq!(vault.clears.get(), 1);
    }

    #[test]
    fn rotated_refresh_survives_profile_or_vault_failure_and_is_used_on_retry() {
        use std::cell::RefCell;
        struct FakeVault {
            saved: RefCell<Option<String>>,
            fail: bool,
        }
        impl Vault for FakeVault {
            fn load(&self) -> Result<Option<String>, ()> {
                Ok(self.saved.borrow().clone())
            }
            fn save(&self, value: &str) -> Result<(), ()> {
                if self.fail {
                    return Err(());
                }
                *self.saved.borrow_mut() = Some(value.into());
                Ok(())
            }
            fn clear(&self) -> Result<(), ()> {
                panic!("transient failure must not clear credentials")
            }
        }
        struct RotationProvider;
        impl SyncHttpClient for RotationProvider {
            type Error = std::io::Error;
            fn call(
                &self,
                request: oauth2::HttpRequest,
            ) -> Result<oauth2::HttpResponse, Self::Error> {
                let fields: std::collections::HashMap<_, _> =
                    url::form_urlencoded::parse(request.body()).collect();
                assert_eq!(
                    fields.get("refresh_token").map(|s| s.as_ref()),
                    Some("rotated-refresh")
                );
                Ok(oauth2::http::Response::builder().status(200).header("content-type", "application/json")
                    .body(br#"{"access_token":"next-access","token_type":"Bearer","refresh_token":"next-refresh","expires_in":3600}"#.to_vec()).unwrap())
            }
        }
        for fail in [false, true] {
            let shared = Shared {
                status: Mutex::new(AuthStatus::Disconnected),
                intent: Mutex::new(None),
                generation: AtomicU64::new(0),
                completion: AtomicU64::new(0),
                stopped: AtomicBool::new(false),
            };
            let vault = FakeVault {
                saved: RefCell::new(Some("old-refresh".into())),
                fail,
            };
            let token: BasicTokenResponse = serde_json::from_str(r#"{"access_token":"access","token_type":"Bearer","refresh_token":"rotated-refresh","expires_in":3600}"#).unwrap();
            let result = finish_token(token, None, &vault, &shared, 0, |_| {
                assert!(!fail, "vault failure must be handled before profile I/O");
                assert_eq!(vault.load().unwrap().as_deref(), Some("rotated-refresh"));
                Err(Failure::Message("temporary profile failure"))
            });
            let Err(Failure::Retained { refresh, .. }) = result else {
                panic!("replacement credential must be retained");
            };
            let session = Session::retry(refresh, None);
            assert!(
                session.account.is_none(),
                "unverified identity must not authorize presence"
            );
            let next = client(DEFAULT_APPLICATION_ID)
                .exchange_refresh_token(&session.refresh)
                .request(&RotationProvider)
                .unwrap();
            assert_eq!(next.refresh_token().unwrap().secret(), "next-refresh");
        }
    }

    #[test]
    fn callback_accepts_only_exact_loopback_target_and_expected_state() {
        let request = b"GET /discord/callback?code=test-code&state=correct HTTP/1.1\r\nHost: 127.0.0.1:45961\r\n\r\n";
        assert!(
            matches!(parse_callback(request, "correct"), Ok(Callback::Code(code)) if code.secret() == "test-code")
        );
        assert!(parse_callback(request, "other-state").is_err());
        assert!(parse_callback(b"GET /discord/callback?code=x&state=correct HTTP/1.1\r\nHost: attacker.example\r\n\r\n", "correct").is_err());
        assert!(parse_callback(b"POST /discord/callback?code=x&state=correct HTTP/1.1\r\nHost: 127.0.0.1:45961\r\n\r\n", "correct").is_err());
    }

    #[test]
    fn callback_rejects_duplicate_fields_fragments_and_unbounded_input() {
        for target in [
            "/discord/callback?code=a&code=b&state=s",
            "/discord/callback?code=a&state=s&state=s",
            "/different?code=a&state=s",
            "http://attacker.example/discord/callback?code=a&state=s",
            "/discord/callback?code=a&state=s#fragment",
        ] {
            let request = format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1:45961\r\n\r\n");
            assert!(parse_callback(request.as_bytes(), "s").is_err());
        }
        assert!(parse_callback(&vec![b'x'; MAX_HEADER_BYTES + 1], "s").is_err());
    }

    #[test]
    fn provider_error_is_handled_without_returning_error_description() {
        let request = b"GET /discord/callback?error=access_denied&error_description=private&state=s HTTP/1.1\r\nHost: 127.0.0.1:45961\r\n\r\n";
        assert!(matches!(parse_callback(request, "s"), Ok(Callback::Denied)));
    }

    #[test]
    fn pkce_url_contains_only_identify_scope_and_never_verifier() {
        let client = client(DEFAULT_APPLICATION_ID);
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let (url, state) = client
            .authorize_url(CsrfToken::new_random)
            .add_scope(Scope::new("identify".into()))
            .set_pkce_challenge(challenge)
            .url();
        let query: std::collections::HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(query.get("scope").map(|v| v.as_ref()), Some("identify"));
        assert_eq!(
            query.get("code_challenge_method").map(|v| v.as_ref()),
            Some("S256")
        );
        assert_eq!(
            query.get("redirect_uri").map(|v| v.as_ref()),
            Some(REDIRECT_URI)
        );
        assert_eq!(
            query.get("state").map(|v| v.as_ref()),
            Some(state.secret().as_str())
        );
        assert!(!url.as_str().contains(verifier.secret()));
    }

    #[test]
    fn invalid_profile_ids_are_rejected_and_display_text_is_sanitized() {
        assert!(parse_account(br#"{"id":"not-an-id","username":"test"}"#).is_err());
        let profile = parse_account(
            br#"{"id":"123456789012345678","global_name":"Player\nOne","username":"fallback"}"#,
        )
        .unwrap();
        assert_eq!(profile.display_name, "PlayerOne");
    }

    #[test]
    fn public_token_exchange_has_verifier_and_no_client_secret_or_basic_auth() {
        struct FakeProvider;
        impl SyncHttpClient for FakeProvider {
            type Error = std::io::Error;
            fn call(
                &self,
                request: oauth2::HttpRequest,
            ) -> Result<oauth2::HttpResponse, Self::Error> {
                assert_eq!(request.uri().to_string(), TOKEN_URL);
                assert!(!request.headers().contains_key("authorization"));
                let fields: std::collections::HashMap<_, _> =
                    url::form_urlencoded::parse(request.body()).collect();
                assert_eq!(
                    fields.get("client_id").map(|s| s.as_ref()),
                    Some(DEFAULT_APPLICATION_ID)
                );
                assert_eq!(
                    fields.get("code_verifier").map(|s| s.as_ref()),
                    Some("test-verifier")
                );
                assert!(!fields.contains_key("client_secret"));
                Ok(oauth2::http::Response::builder().status(200).header("content-type", "application/json").body(br#"{"access_token":"test-access","token_type":"Bearer","expires_in":3600,"refresh_token":"test-refresh"}"#.to_vec()).unwrap())
            }
        }
        let token = client(DEFAULT_APPLICATION_ID)
            .exchange_code(AuthorizationCode::new("test-code".into()))
            .set_pkce_verifier(oauth2::PkceCodeVerifier::new("test-verifier".into()))
            .request(&FakeProvider)
            .unwrap();
        assert_eq!(token.refresh_token().unwrap().secret(), "test-refresh");
    }

    #[test]
    fn cancellation_cannot_publish_a_previous_account() {
        let shared = Shared {
            status: Mutex::new(AuthStatus::Disconnected),
            intent: Mutex::new(None),
            generation: AtomicU64::new(1),
            completion: AtomicU64::new(0),
            stopped: AtomicBool::new(false),
        };
        shared.set_status(
            0,
            AuthStatus::Connected(DiscordAccount {
                id: "123456789012345678".into(),
                display_name: "Old account".into(),
                avatar_url: None,
            }),
        );
        assert_eq!(*shared.status.lock().unwrap(), AuthStatus::Disconnected);
    }

    #[test]
    fn oauth_secret_debug_output_is_redacted() {
        let secret = AuthorizationCode::new("not-a-real-authorization-code".into());
        assert!(!format!("{secret:?}").contains("not-a-real-authorization-code"));
    }
}
