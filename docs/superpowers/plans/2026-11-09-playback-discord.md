# Console Link Playback and Discord Implementation Plan

> **For agentic workers:** Work in owned modules, preserve media lifecycle, then independent review and native verification.

**Goal:** Automatic capture defaults, integrated gameplay/overlay controls and real Discord account linking.

**Architecture:** Native media remains unchanged; add deterministic mode policy and presentation state machine. OAuth identity is separate from local IPC presence; account IDs must match.

**Tech Stack:** Existing Rust/egui stack, standard OAuth2 PKCE, native credential store.

**Spec:** ../specs/2026-11-09-playback-discord-design.md

## Global Constraints

- Do not invent 60 FPS when hardware does not support it.
- Screenshots contain only the game frame, even when overlay is open.
- No global OS keyboard hooks; no letter shortcuts while typing.
- No Client Secret, plaintext tokens, callback codes or sensitive URL logs.
- Registration/consent and actual login cannot be replaced with mocked success.

## Review Focus

- Old cached 30 FPS must not override new automatic defaults.
- Closing gameplay must restore ordinary UI without stopping the entire application.
- Fullscreen transitions must restore size/decorations safely on Mac and Windows.
- Auth cancellation/invalid state must not leave a server or credential writing thread running.
- OAuth account mismatch must block activity on the other desktop account.

### Tasks

- [x] Implement mode policy with RED/GREEN tests and rational FPS.
- [x] Extend additive settings defaults for automatic capture and window mode.
- [x] Implement focused shortcuts, presentation state machine and overlay; check real renderer snapshots.
- [x] Register Discord Application with owner, Public Client and redirect URI.
- [x] Implement PKCE worker, credential store, account-matched IPC and callback tests.
- [x] Integrate auth UI after gameplay changes stabilize.
- [ ] Validate actual owner consent/code exchange; owner has not tested yet.
- [ ] Independently review, run native checks and publish a downloadable updated build.

Game recognition is a documented proposal; no inert feature toggle or fake automatic recognition is shipped.
