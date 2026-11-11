use crate::config::discord as config;
use std::io::{Read, Write};
#[cfg(windows)]
use std::time::{Duration, Instant};

pub(super) fn decode_header(header: [u8; 8]) -> std::io::Result<(u32, usize)> {
    let opcode = u32::from_le_bytes(header[..4].try_into().expect("four bytes"));
    let length = u32::from_le_bytes(header[4..].try_into().expect("four bytes")) as usize;
    if length > config::MAX_PACKET {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Discord packet exceeds limit",
        ));
    }
    Ok((opcode, length))
}

#[cfg(unix)]
pub(super) struct Transport(pub(super) std::os::unix::net::UnixStream);

#[cfg(unix)]
impl Transport {
    pub(super) fn connect() -> std::io::Result<Self> {
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
                    stream.set_read_timeout(Some(config::IO_TIMEOUT))?;
                    stream.set_write_timeout(Some(config::IO_TIMEOUT))?;
                    return Ok(Self(stream));
                }
            }
        }
        Err(std::io::ErrorKind::NotFound.into())
    }
    pub(super) fn read_exact(&mut self, bytes: &mut [u8]) -> std::io::Result<()> {
        self.0.read_exact(bytes)
    }
    pub(super) fn write_all(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.0.write_all(bytes)
    }
}

#[cfg(windows)]
pub(super) struct Transport(pub(super) std::fs::File);

#[cfg(windows)]
impl Transport {
    pub(super) fn connect() -> std::io::Result<Self> {
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
    pub(super) fn read_exact(&mut self, mut bytes: &mut [u8]) -> std::io::Result<()> {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::{Foundation::HANDLE, System::Pipes::PeekNamedPipe};
        let deadline = Instant::now() + config::IO_TIMEOUT;
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
    pub(super) fn write_all(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        let deadline = Instant::now() + config::IO_TIMEOUT;
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
