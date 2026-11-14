//! Pinned public download metadata for the private Nintendo helper runtime.

pub const NODE_VERSION: &str = "v24.22.3";
pub const NXAPI_VERSION: &str = "1.6.1-next.257";
pub const REVISION: &str = "v1";

#[derive(Clone, Copy)]
pub struct PlatformSpec {
    pub url: &'static str,
    pub sha256: &'static str,
    pub archive_name: &'static str,
    pub archive_root: &'static str,
    pub node_relative: &'static str,
    pub npm_relative: &'static str,
}

pub fn platform() -> Result<PlatformSpec, String> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        Ok(PlatformSpec {
            url: "https://nodejs.org/dist/v24.22.3/node-v24.22.3-darwin-arm64.tar.gz",
            sha256: "ff6e17d4874b76d272212b908cc020d6e815fba5a6c7eaaa06add725840307fd",
            archive_name: "node-v24.22.3-darwin-arm64.tar.gz",
            archive_root: "node-v24.22.3-darwin-arm64",
            node_relative: "bin/node",
            npm_relative: "lib/node_modules/npm/bin/npm-cli.js",
        })
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        Ok(PlatformSpec {
            url: "https://nodejs.org/dist/v24.22.3/node-v24.22.3-win-x64.zip",
            sha256: "e612af99f626ab415ea225af6bce5c971feb448cb5cae250bb1c3fa50c5f7d9f",
            archive_name: "node-v24.22.3-win-x64.zip",
            archive_root: "node-v24.22.3-win-x64",
            node_relative: "node.exe",
            npm_relative: "node_modules/npm/bin/npm-cli.js",
        })
    }
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64")
    )))]
    {
        Err(
            "Установка Nintendo-компонентов поддерживается на Mac Apple Silicon и Windows x64."
                .into(),
        )
    }
}
