# Nintendo presence integration

Use the already validated local nxapi secondary-account login. Product code contains no account data. Credentials stay in local nxapi storage (OS application data, or the explicitly ignored `.local/nintendo-helper` development directory); only opt-in and target friend ID persist in the normal local settings store. Never log raw helper stdout/stderr, Nintendo tokens, auth URLs, or real response fixtures.

Implement a bounded background subprocess adapter (Node + installed nxapi, no new Rust dependency), list friends, select target explicitly (auto-select only when exactly one friend), poll every60s, clear stale/offline/error presence. Keep manual game unchanged. Nintendo status can publish while capture is stopped, but only with existing Discord toggle/account enabled. Same effective title feeds UI, window title and Discord; timer restarts only on game change.

Alternatives: direct Nintendo auth duplicates unstable nxapi and credential handling; public presence service adds third-party disclosure. Local nxapi already works and is selected from the preceding user setup.

1. Add tested filtered protocol/state in src/nintendo.rs and scripts/nintendo-presence.cjs. All fixtures synthetic. Prove offline/error clears title and parser rejects malformed/unbounded input.
2. Wire settings, app background polling/selection/status UI and Discord effective title without overwriting manual settings.
3. Validate full Rust tests, Clippy/format, Node bridge tests and live local status (no raw data saved in repo), release build and UI. Scan diff for credentials; do not commit or push without request.

No account registration/auth wizard or bundled Node/npm runtime in this slice. Existing local helper is a prerequisite, with an explicit unavailable message elsewhere.


Validation: 90 library +37 app tests passed;4 Node protocol tests passed. Strict Clippy, formatting and release compilation passed. The opt-in live helper test passed without dumping account data. The real application displayed the Nintendo title and artwork and reported Discord acknowledgement after restarting outside the restricted execution environment.

Privacy checks found no current credential values in 116 repository candidate files or 251 historical blobs. The ignored development credential directory is not encrypted and must not be included in project archives. Packaging uses an explicit file list and excludes account data and helper dependencies.

Local Windows packaging uses the GNU release build and existing RTX runtime files with the repository Inno Setup script. The CI workflow separately builds MSVC packages and runs installer smoke tests on a disposable runner. Local installation is left to the user.
