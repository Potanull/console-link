# Rust Module Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Organize Rust sources by responsibility, remove proven duplication, and centralize operational constants.

**Architecture:** Keep the library and binary boundaries. Group code under `app`, `capture`, `audio`, `discord`, `recognition`, and `platform`. Use typed constants in `config.rs` and small shared functions rather than new traits or dependencies.

**Tech Stack:** Rust 2024, Cargo, existing dependencies.

**Spec:** User decisions in this session: public Rust API may change; preserve application behavior, settings formats, and Windows/macOS support; use `config.rs`; do not create a DOCX.

## Execution ledger

- Approved by the user on 2026-11-11 for implementation in the current session and workspace.
- Ruling: work in the existing checkout without commits or branch integration; the user approved implementation in their active workspace.
- Pre-flight: Task 2 consumes the new paths and facades from Task 1; Task 3 documents and checks both. No conflicting interfaces.
- Baseline: Cargo, rustc, and rustfmt were absent. WSL is not installed; Docker daemon is unavailable. Preparing official, SHA-256-verified Rust tools in a task-specific temporary directory.
- Ruling: preserve both platform-specific frame-buffer validators and share only BGRA row conversion. This removes repetition without changing signed-pitch or buffer error behavior.
- Ruling: retain existing regression tests for behavior-preserving moves and extraction; do not introduce tests that merely duplicate module wiring or constants.
- Task 1: complete. Sources grouped by ownership; imports, facades, and manifest-relative asset includes updated.
- Task 2: complete. Operational constants centralized; shared CLI lookup, icon decoding, Discord ID validation, and BGRA row conversion. App lifecycle/pages/presentation/widgets, recognition catalog/models, and Discord transport separated.
- Task 3: static verification complete. `cargo fmt --all -- --check`, `cargo metadata --no-deps --format-version 1`, `git diff --check`, module/asset resolution, original comment preservation, and retention of all 91 original tests passed. `rustc --test --emit=metadata` checked the actual config and pixel conversion code plus its three tests.
- Execution limitation: native test linking fails because MSVC `link.exe` is absent. Full offline Cargo tests also stop at uncached `cpal`; full compilation, Clippy, and test execution remain unverified. No system toolchain or Build Tools were installed; official Rust tools reside in the task temporary directory.
- Concurrent work: unrelated controls plans and two equalizer regression tests appeared during implementation. Preserve these changes; they are outside this refactor's verification claims.
- Size accounting: before the final formatting pass, Rust sources grew from 8,992 to 9,201 lines including concurrent tests. Shared logic has less duplication, but configuration declarations and module boundaries increase total source lines. Do not claim a net line reduction.

## Global Constraints

- Preserve defaults, validation, ordering, threading, platform guards, and existing comments.
- Do not add dependencies, external configuration formats, features, or commits.
- Update imports and embedded asset paths together with moves.
- Keep existing tests with the code they exercise.

## Review Focus

- Embedded assets must resolve after moving source files.
- Both binary and library must use the same configuration definitions.
- Discord ID validation must retain its exact accepted range and ASCII rules.
- Native capture must preserve signed pitch, row orientation, and alpha semantics.
- Settings serialization and platform-specific behavior must remain unchanged.

### Task 1: Establish verification and reorganize modules

**Files:** `src/lib.rs`, `src/main.rs`, all moved Rust sources.

**Interfaces:** Library exports `capture`, `audio`, `discord`, `recognition`, `settings`, and `config`. Binary owns `app` and `platform`.

- [ ] Locate Rust tools and run baseline `cargo test --all-targets --locked` and `cargo fmt --all -- --check` when available.
- [ ] Move `app.rs` to `app/mod.rs`; group artwork, playback, shortcuts, and theme under `app/`.
- [ ] Group native backends, video types, policy, history, and session helpers under `capture/` with an explicit facade.
- [ ] Move `audio.rs` to `audio/mod.rs`, `discord.rs` to `discord/mod.rs`, `discord_auth.rs` to `discord/auth.rs`, and `game_recognition.rs` to `recognition/mod.rs`.
- [ ] Group app return and window geometry under binary `platform/`; update all module paths and asset includes.
- [ ] Run compilation and existing tests when tools are available; otherwise record exact verification limitations.

### Task 2: Centralize constants and remove duplication

**Files:** `src/config.rs`, `src/main.rs`, `src/app/`, `src/discord/`, `src/recognition/`, `src/capture/`.

**Interfaces:** Typed configuration constants grouped by subsystem; shared Discord `valid_id(&str) -> bool`; shared argument and icon helpers in the binary.

- [ ] Move application defaults, resource budgets, service endpoints, and major timeouts into subsystem sections in `config.rs`, preserving exact values.
- [ ] Keep protocol flags, platform API constants, test fixtures, and visual theme definitions near their owners.
- [ ] Reuse Discord ID validation, command-line value lookup, and icon decoding without changing behavior.
- [ ] Inspect BGRA conversions; share only if platform-specific semantics can be preserved and existing regression coverage can prove them.
- [ ] Split oversized modules along cohesive responsibilities where this reduces navigation cost without adding pass-through abstractions.

### Task 3: Verify and document the result

**Files:** `docs/architecture.md`, affected Rust sources.

**Interfaces:** Current architecture documentation matches actual module ownership.

- [ ] Update current architecture documentation; leave historical plans intact.
- [ ] Run `cargo fmt --all -- --check`, `cargo test --all-targets --locked`, and appropriate Windows/macOS checks when available.
- [ ] Run `git diff --check`, inspect module resolution, embedded assets, remaining old imports, and the complete diff.
- [ ] Report actual checks, code reduction excluding file moves, and any environment limitations.
