//! Explicit, bounded capture-start tracing. Never record media, settings or credentials.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

const MAX_LOG_BYTES: usize = 64 * 1024;
const MAX_STAGE_CHARS: usize = 48;
const MAX_DETAIL_CHARS: usize = 512;

struct LogFile {
    file: File,
    bytes: usize,
    stopped: bool,
}

pub(super) struct ConnectionDiagnostics {
    started: Instant,
    output: Mutex<LogFile>,
}

impl ConnectionDiagnostics {
    /// Creates a new log only when explicitly requested by the diagnostic CLI.
    /// Existing files, including symlinks, are never followed or overwritten.
    pub(super) fn new(path: PathBuf) -> Result<Self, String> {
        if !path.is_absolute() {
            return Err("Connection diagnostic log path must be absolute.".into());
        }
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(path)
            .map_err(|error| format!("Cannot create connection diagnostic log: {error}"))?;
        let header = format!(
            "Console Link {} connection diagnostics; capture startup only; no media or credentials\n",
            env!("CARGO_PKG_VERSION")
        );
        file.write_all(header.as_bytes())
            .map_err(|error| format!("Cannot write connection diagnostic log: {error}"))?;
        Ok(Self {
            started: Instant::now(),
            output: Mutex::new(LogFile {
                file,
                bytes: header.len(),
                stopped: false,
            }),
        })
    }

    /// Callers supply only stage names and capture-start errors/dimensions.
    /// I/O failures disable tracing without changing capture or UI behavior.
    pub(super) fn record(&self, stage: &str, detail: &str) {
        let stage: String = stage
            .chars()
            .take(MAX_STAGE_CHARS)
            .map(|character| {
                if character.is_ascii_alphanumeric() || matches!(character, '_' | '-') {
                    character
                } else {
                    '_'
                }
            })
            .collect();
        let detail: String = detail
            .chars()
            .take(MAX_DETAIL_CHARS)
            .map(|character| {
                if character.is_control() {
                    ' '
                } else {
                    character
                }
            })
            .collect();
        let line = format!(
            "{:>8}ms {stage}: {detail}\n",
            self.started.elapsed().as_millis()
        );
        let mut output = self
            .output
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if output.stopped {
            return;
        }
        if output.bytes.saturating_add(line.len()) > MAX_LOG_BYTES {
            let marker = b"trace_limit: connection diagnostic log reached 64 KiB\n";
            if output.bytes.saturating_add(marker.len()) <= MAX_LOG_BYTES {
                let _ = output.file.write_all(marker);
            }
            output.stopped = true;
            return;
        }
        if output.file.write_all(line.as_bytes()).is_err() {
            output.stopped = true;
            return;
        }
        output.bytes += line.len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn path() -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "console-link-connection-test-{}-{}-{}.log",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn diagnostic_log_does_not_overwrite_existing_files() {
        let path = path();
        std::fs::write(&path, b"existing user file").unwrap();
        assert!(ConnectionDiagnostics::new(path.clone()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"existing user file");
        std::fs::remove_file(path).unwrap();
        assert!(ConnectionDiagnostics::new(PathBuf::from("relative.log")).is_err());
    }

    #[test]
    fn diagnostic_lines_cannot_inject_extra_records() {
        let path = path();
        let log = ConnectionDiagnostics::new(path.clone()).unwrap();
        log.record("video\nstart", "first line\nsecond line\r\n\0");
        drop(log);
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 2);
        assert!(text.contains("video_start: first line second line"));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn diagnostic_output_stays_bounded() {
        let path = path();
        let log = ConnectionDiagnostics::new(path.clone()).unwrap();
        for _ in 0..1000 {
            log.record("capture_error", &"x".repeat(1000));
        }
        drop(log);
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.len() <= MAX_LOG_BYTES);
        assert!(String::from_utf8(bytes).unwrap().contains("trace_limit:"));
        std::fs::remove_file(path).unwrap();
    }
}
