//! Per-user Nintendo components. Account data stays outside the runtime tree.

use super::runtime_config::{self, PlatformSpec};
use oauth2::reqwest;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const PACKAGE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/packaging/nintendo/package.json"
));
const PACKAGE_LOCK: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/packaging/nintendo/package-lock.json"
));
const MAX_DOWNLOAD: u64 = 128 * 1024 * 1024;
const CANCELLED: &str = "Установка отменена.";
const DISK_ERROR: &str =
    "Не удалось подготовить компоненты Nintendo. Проверь права и свободное место.";

/// Cancellation owns the active child so UI teardown can stop it before exiting.
#[derive(Default)]
pub struct Cancellation {
    cancelled: AtomicBool,
    child: Mutex<Option<Child>>,
}

impl Cancellation {
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        Self::stop_locked(&mut self.child());
    }

    fn child(&self) -> MutexGuard<'_, Option<Child>> {
        self.child
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn stop_locked(child: &mut Option<Child>) {
        if let Some(mut child) = child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for Cancellation {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Downloading,
    Extracting,
    Installing,
    Verifying,
}

impl Stage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Downloading => "Загружаем Node.js…",
            Self::Extracting => "Распаковываем компоненты…",
            Self::Installing => "Устанавливаем nxapi…",
            Self::Verifying => "Проверяем компоненты Nintendo…",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ManagedHelper {
    pub node: PathBuf,
    pub package_directory: PathBuf,
}

fn layout(root: &Path, spec: &PlatformSpec) -> ManagedHelper {
    ManagedHelper {
        node: root.join(spec.archive_root).join(spec.node_relative),
        package_directory: root.join("nxapi"),
    }
}

fn marker() -> String {
    format!(
        "{}\n{}\n{}\n{:x}\n",
        runtime_config::REVISION,
        runtime_config::NODE_VERSION,
        runtime_config::NXAPI_VERSION,
        Sha256::digest(PACKAGE_LOCK)
    )
}

fn read_bounded(path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(std::io::Error::other("Component metadata exceeds limit"));
    }
    Ok(bytes)
}

fn package_is_pinned(package_directory: &Path) -> bool {
    let package = package_directory.join("node_modules/nxapi");
    package.join("bin/nxapi.js").is_file()
        && read_bounded(&package.join("package.json"), 64 * 1024)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .is_some_and(|value| value["version"].as_str().is_some_and(version_is_pinned))
}

fn version_is_pinned(version: &str) -> bool {
    // npm omits SemVer build metadata from the registry/lock version, not from package.json.
    let (base, metadata) = version
        .split_once('+')
        .map_or((version, None), |(base, metadata)| (base, Some(metadata)));
    base == runtime_config::NXAPI_VERSION
        && metadata.is_none_or(|metadata| {
            metadata.split('.').all(|part| {
                !part.is_empty()
                    && part
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            })
        })
}

fn installed_at(root: &Path, spec: &PlatformSpec) -> Option<ManagedHelper> {
    let helper = layout(root, spec);
    (read_bounded(&root.join("complete"), 1024).ok()?.as_slice() == marker().as_bytes()
        && helper.node.is_file()
        && root
            .join(spec.archive_root)
            .join(spec.npm_relative)
            .is_file()
        && package_is_pinned(&helper.package_directory))
    .then_some(helper)
}

/// File checks only. The install/repair action additionally runs the components.
pub fn installed(base: &Path) -> Option<ManagedHelper> {
    let spec = runtime_config::platform().ok()?;
    installed_at(&base.join("runtimes").join(runtime_config::REVISION), &spec)
}

fn check_cancelled(cancelled: &Cancellation) -> Result<(), String> {
    if cancelled.is_cancelled() {
        Err(CANCELLED.into())
    } else {
        Ok(())
    }
}

struct Staging(PathBuf);
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Run on a worker. Only this runtime's private directories are changed.
pub fn install(
    base: &Path,
    progress: impl Fn(Stage),
    cancelled: &Cancellation,
) -> Result<ManagedHelper, String> {
    check_cancelled(cancelled)?;
    if !base.is_absolute() {
        return Err("Для установки нужен абсолютный путь к каталогу компонентов.".into());
    }
    let spec = runtime_config::platform()?;
    let runtimes = base.join("runtimes");
    fs::create_dir_all(&runtimes).map_err(|_| DISK_ERROR)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(runtimes.join("install.lock"))
        .map_err(|_| DISK_ERROR)?;
    // OS locks disappear on process exit, so a crashed install cannot leave a stale lock.
    lock.try_lock()
        .map_err(|_| "Установка Nintendo уже идет в другом окне Console Link.")?;
    let target = runtimes.join(runtime_config::REVISION);
    let backup = runtimes.join(format!(".previous-{}", runtime_config::REVISION));
    if !target.exists() && backup.exists() {
        fs::rename(&backup, &target).map_err(|_| DISK_ERROR)?;
    }
    let staging = Staging(runtimes.join(format!(".install-{}", runtime_config::REVISION)));
    if staging.0.exists() {
        fs::remove_dir_all(&staging.0).map_err(|_| DISK_ERROR)?;
    }
    fs::create_dir(&staging.0).map_err(|_| DISK_ERROR)?;
    if let Some(helper) = installed_at(&target, &spec) {
        progress(Stage::Verifying);
        if verify(&helper, &staging.0, cancelled).is_ok() {
            let _ = fs::remove_dir_all(&backup);
            return Ok(helper);
        }
        check_cancelled(cancelled)?;
    }
    progress(Stage::Downloading);
    let archive = staging.0.join(spec.archive_name);
    download(&spec, &archive, cancelled)?;
    progress(Stage::Extracting);
    extract(&spec, &archive, &staging.0, cancelled)?;
    fs::remove_file(&archive).map_err(|_| DISK_ERROR)?;
    let helper = layout(&staging.0, &spec);
    let npm = staging.0.join(spec.archive_root).join(spec.npm_relative);
    if !helper.node.is_file() || !npm.is_file() {
        return Err("Архив Node.js не содержит ожидаемые компоненты.".into());
    }
    fs::create_dir(&helper.package_directory).map_err(|_| DISK_ERROR)?;
    fs::write(helper.package_directory.join("package.json"), PACKAGE).map_err(|_| DISK_ERROR)?;
    fs::write(
        helper.package_directory.join("package-lock.json"),
        PACKAGE_LOCK,
    )
    .map_err(|_| DISK_ERROR)?;
    let user_config = staging.0.join("empty-user.npmrc");
    let global_config = staging.0.join("empty-global.npmrc");
    fs::write(&user_config, []).map_err(|_| DISK_ERROR)?;
    fs::write(&global_config, []).map_err(|_| DISK_ERROR)?;
    progress(Stage::Installing);
    let mut command = private_command(&helper.node, &helper.node, &staging.0)?;
    command
        .arg(npm)
        .arg("ci")
        .args([
            "--omit=dev",
            "--ignore-scripts",
            "--no-audit",
            "--no-fund",
            "--logs-max=0",
            "--loglevel=error",
            "--global=false",
        ])
        .arg("--userconfig")
        .arg(user_config)
        .arg("--globalconfig")
        .arg(global_config)
        .arg("--cache")
        .arg(staging.0.join("npm-cache"))
        .arg("--registry=https://registry.npmjs.org")
        .current_dir(&helper.package_directory);
    run(command, Duration::from_secs(300), cancelled).map_err(|error| {
        if error == CANCELLED {
            error
        } else {
            "Не удалось установить nxapi. Проверь интернет и повтори установку.".into()
        }
    })?;
    progress(Stage::Verifying);
    verify(&helper, &staging.0, cancelled)?;
    check_cancelled(cancelled)?;
    fs::write(staging.0.join("complete"), marker()).map_err(|_| DISK_ERROR)?;
    let _ = fs::remove_dir_all(staging.0.join("npm-cache"));
    let _ = fs::remove_dir_all(staging.0.join(".verify-data"));
    let _ = fs::remove_dir_all(staging.0.join("tmp"));
    activate(&staging.0, &target, &backup)?;
    installed_at(&target, &spec)
        .ok_or_else(|| "Установка компонентов Nintendo не прошла проверку.".into())
}

fn download(
    spec: &PlatformSpec,
    destination: &Path,
    cancelled: &Cancellation,
) -> Result<(), String> {
    const ERROR: &str = "Не удалось загрузить Node.js. Проверь интернет и повтори установку.";
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|_| ERROR)?;
    let mut response = client
        .get(spec.url)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|_| ERROR)?;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_DOWNLOAD)
    {
        return Err("Архив Node.js превышает допустимый размер.".into());
    }
    let mut file = File::create(destination).map_err(|_| DISK_ERROR)?;
    let mut hash = Sha256::new();
    let mut count = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        check_cancelled(cancelled)?;
        let length = response.read(&mut buffer).map_err(|_| ERROR)?;
        if length == 0 {
            break;
        }
        count += length as u64;
        if count > MAX_DOWNLOAD {
            return Err("Архив Node.js превышает допустимый размер.".into());
        }
        hash.update(&buffer[..length]);
        file.write_all(&buffer[..length]).map_err(|_| DISK_ERROR)?;
    }
    verify_digest(&format!("{:x}", hash.finalize()), spec.sha256)
}

fn verify_digest(actual: &str, expected: &str) -> Result<(), String> {
    if actual == expected {
        Ok(())
    } else {
        Err("Контрольная сумма Node.js не совпала. Повтори установку.".into())
    }
}

fn extract(
    spec: &PlatformSpec,
    archive: &Path,
    destination: &Path,
    cancelled: &Cancellation,
) -> Result<(), String> {
    #[cfg(windows)]
    let program = system_root()?.join("System32/tar.exe");
    #[cfg(not(windows))]
    let program = PathBuf::from("/usr/bin/tar");
    let node = destination.join(spec.archive_root).join(spec.node_relative);
    let mut command = private_command(&program, &node, destination)?;
    #[cfg(windows)]
    command.arg("-xf");
    #[cfg(not(windows))]
    command.arg("-xzf");
    command.arg(archive).arg("-C").arg(destination);
    run(command, Duration::from_secs(120), cancelled).map_err(|error| {
        if error == CANCELLED {
            error
        } else {
            "Не удалось распаковать Node.js. Повтори установку.".into()
        }
    })
}

#[cfg(windows)]
fn system_root() -> Result<PathBuf, String> {
    std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .ok_or_else(|| "Не найден системный каталог Windows.".into())
}

fn private_command(program: &Path, node: &Path, work: &Path) -> Result<Command, String> {
    let temporary = work.join("tmp");
    fs::create_dir_all(&temporary).map_err(|_| DISK_ERROR)?;
    let mut paths = vec![node.parent().ok_or(DISK_ERROR)?.to_owned()];
    #[cfg(windows)]
    let system_root = system_root()?;
    #[cfg(windows)]
    paths.extend([system_root.join("System32"), system_root.clone()]);
    #[cfg(not(windows))]
    paths.extend([PathBuf::from("/usr/bin"), PathBuf::from("/bin")]);
    let mut command = Command::new(program);
    command
        .current_dir(work)
        .env_clear()
        .env("PATH", std::env::join_paths(paths).map_err(|_| DISK_ERROR)?)
        .env("TMP", &temporary)
        .env("TEMP", &temporary)
        .env("TMPDIR", &temporary)
        .env("NXAPI_DEBUG_FILE", "0")
        .env("DEBUG", "")
        .env("NXAPI_SKIP_UPDATE_CHECK", "1")
        .stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command
            .env("SystemRoot", system_root)
            .creation_flags(0x08000000);
    }
    Ok(command)
}

fn run(mut command: Command, timeout: Duration, cancelled: &Cancellation) -> Result<(), String> {
    command.stdout(Stdio::null()).stderr(Stdio::null());
    {
        let mut child = cancelled.child();
        check_cancelled(cancelled)?;
        if child.is_some() {
            return Err("Компоненты Nintendo уже выполняют другую проверку.".into());
        }
        *child = Some(
            command
                .spawn()
                .map_err(|_| "Не удалось запустить компоненты Nintendo.")?,
        );
    }
    let start = Instant::now();
    loop {
        {
            let mut child = cancelled.child();
            if cancelled.is_cancelled() {
                Cancellation::stop_locked(&mut child);
                return Err(CANCELLED.into());
            }
            let result = child.as_mut().ok_or(CANCELLED)?.try_wait();
            match result {
                Ok(Some(status)) => {
                    if let Some(mut finished) = child.take() {
                        let _ = finished.wait();
                    }
                    check_cancelled(cancelled)?;
                    return if status.success() {
                        Ok(())
                    } else {
                        Err("Компоненты Nintendo завершились с ошибкой.".into())
                    };
                }
                Ok(None) if start.elapsed() < timeout => {}
                _ => {
                    Cancellation::stop_locked(&mut child);
                    check_cancelled(cancelled)?;
                    return Err("Установка компонентов Nintendo не завершилась вовремя.".into());
                }
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn verify(helper: &ManagedHelper, work: &Path, cancelled: &Cancellation) -> Result<(), String> {
    if !package_is_pinned(&helper.package_directory) {
        return Err("Версия nxapi не соответствует закрепленной версии приложения.".into());
    }
    let mut node = private_command(&helper.node, &helper.node, work)?;
    node.arg("--eval").arg(format!(
        "if(process.version !== {}) process.exit(1)",
        serde_json::to_string(runtime_config::NODE_VERSION).expect("Static version is JSON text")
    ));
    run(node, Duration::from_secs(15), cancelled).map_err(|error| {
        if error == CANCELLED {
            error
        } else {
            "Версия Node.js не соответствует закрепленной версии приложения.".into()
        }
    })?;
    let mut nxapi = private_command(&helper.node, &helper.node, work)?;
    nxapi
        .arg(
            helper
                .package_directory
                .join("node_modules/nxapi/bin/nxapi.js"),
        )
        .arg("--data-path")
        .arg(work.join(".verify-data"))
        .args(["nso", "friends", "--help"]);
    run(nxapi, Duration::from_secs(30), cancelled)
}

fn activate(staging: &Path, destination: &Path, backup: &Path) -> Result<(), String> {
    if destination.exists() {
        if backup.exists() {
            fs::remove_dir_all(backup).map_err(|_| DISK_ERROR)?;
        }
        fs::rename(destination, backup).map_err(|_| DISK_ERROR)?;
    }
    if fs::rename(staging, destination).is_err() {
        if backup.exists() {
            fs::rename(backup, destination)
                .map_err(|_| "Не удалось восстановить предыдущие компоненты Nintendo.")?;
        }
        return Err(DISK_ERROR.into());
    }
    let _ = fs::remove_dir_all(backup);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    struct Temporary(PathBuf);
    impl Temporary {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "console-link-runtime-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Temporary {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn corrupted_download_and_cancelled_install_do_not_activate_components() {
        assert!(verify_digest("bad", "expected").is_err());
        let temporary = Temporary::new();
        let base = temporary.0.join("not-created");
        let cancellation = Cancellation::default();
        cancellation.cancel();
        let result = install(
            &base,
            |_| panic!("Cancelled install must not start"),
            &cancellation,
        );
        assert!(result.is_err());
        assert!(!base.exists());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn cancellation_stops_and_reaps_the_active_child_before_returning() {
        use std::sync::Arc;
        unsafe extern "C" {
            fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
        }
        let cancellation = Arc::new(Cancellation::default());
        let worker_cancellation = Arc::clone(&cancellation);
        let worker = std::thread::spawn(move || {
            let mut command = Command::new("/bin/sleep");
            command.arg("30");
            run(command, Duration::from_secs(60), &worker_cancellation)
        });
        let started = Instant::now();
        let pid = loop {
            if let Some(child) = cancellation.child().as_ref() {
                break child.id();
            }
            assert!(started.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(1));
        };
        cancellation.cancel();
        assert!(cancellation.child().is_none());
        assert_eq!(worker.join().unwrap().unwrap_err(), CANCELLED);
        assert!(started.elapsed() < Duration::from_secs(10));
        let mut status = 0;
        // SAFETY: pid belongs to this test's child; 1 is WNOHANG on macOS.
        assert_eq!(unsafe { waitpid(pid as i32, &mut status, 1) }, -1);
    }

    #[test]
    fn npm_build_metadata_does_not_change_the_pinned_prerelease() {
        assert!(version_is_pinned(runtime_config::NXAPI_VERSION));
        assert!(version_is_pinned("1.6.1-next.257+sha.47c9d35"));
        for version in [
            "1.6.1-next.258+sha.47c9d35",
            "1.6.1-next.257-other+sha.47c9d35",
            "1.6.1-next.257+",
            "1.6.1-next.257+sha..47c9d35",
        ] {
            assert!(!version_is_pinned(version));
        }
    }

    #[test]
    fn partial_or_wrong_version_runtime_is_not_reported_as_installed() {
        let temporary = Temporary::new();
        let spec = PlatformSpec {
            url: "https://nodejs.org/synthetic",
            sha256: "synthetic",
            archive_name: "synthetic.tar.gz",
            archive_root: "node",
            node_relative: "bin/node",
            npm_relative: "lib/npm.js",
        };
        let helper = layout(&temporary.0, &spec);
        fs::create_dir_all(helper.node.parent().unwrap()).unwrap();
        fs::write(&helper.node, b"synthetic node").unwrap();
        let npm = temporary.0.join(spec.archive_root).join(spec.npm_relative);
        fs::create_dir_all(npm.parent().unwrap()).unwrap();
        fs::write(npm, b"synthetic npm").unwrap();
        let package = helper.package_directory.join("node_modules/nxapi");
        fs::create_dir_all(package.join("bin")).unwrap();
        fs::write(package.join("bin/nxapi.js"), b"synthetic CLI").unwrap();
        let package_json = package.join("package.json");
        fs::write(
            &package_json,
            serde_json::json!({"version": runtime_config::NXAPI_VERSION}).to_string(),
        )
        .unwrap();
        assert!(installed_at(&temporary.0, &spec).is_none());
        fs::write(temporary.0.join("complete"), marker()).unwrap();
        assert!(installed_at(&temporary.0, &spec).is_some());
        fs::write(package_json, br#"{"version":"unexpected"}"#).unwrap();
        assert!(installed_at(&temporary.0, &spec).is_none());
    }

    #[test]
    fn failed_activation_restores_previous_runtime_and_keeps_account_data() {
        let temporary = Temporary::new();
        let destination = temporary.0.join("runtime");
        let backup = temporary.0.join("previous");
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("old"), b"old runtime").unwrap();
        let data = temporary.0.join("data");
        fs::create_dir(&data).unwrap();
        fs::write(data.join("synthetic-account"), b"synthetic data").unwrap();
        assert!(activate(&temporary.0.join("missing-staging"), &destination, &backup).is_err());
        assert_eq!(fs::read(destination.join("old")).unwrap(), b"old runtime");
        assert_eq!(
            fs::read(data.join("synthetic-account")).unwrap(),
            b"synthetic data"
        );
        assert!(!backup.exists());
    }

    #[test]
    fn success_activates_complete_tree_without_touching_data() {
        let temporary = Temporary::new();
        let staging = temporary.0.join("staging");
        let destination = temporary.0.join("runtime");
        let backup = temporary.0.join("previous");
        fs::create_dir(&staging).unwrap();
        fs::write(staging.join("new"), b"new runtime").unwrap();
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("old"), b"old runtime").unwrap();
        fs::create_dir(temporary.0.join("data")).unwrap();
        activate(&staging, &destination, &backup).unwrap();
        assert_eq!(fs::read(destination.join("new")).unwrap(), b"new runtime");
        assert!(!backup.exists());
        assert!(temporary.0.join("data").is_dir());
    }
}
