# Upscaling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Real NIS, FSR 1 and RTX Video modes with live GPU playback and matching upscaled PNG export.
**Architecture:** Persistent GPU spatial pipelines feed existing presentation. A typed frame recipe snapshots effective mode, color and physical output size. RTX uses genuine SDK native interop once available, never a substitute filter.
**Tech Stack:** Rust, wgpu 27, WGSL, eframe 0.33, NVIDIA RTX Video SDK DX12.
**Spec:** docs/superpowers/specs/2026-11-11-upscaling-design.md (approved by user).

## Global Constraints
- Windows NVIDIA RTX is the current target. No extra capture queue or routine CPU pixel transform/readback.
- NIS and FSR 1 must implement upstream algorithms, retain attribution and validate against references.
- Original export preserves ungraded source dimensions; processed export matches physical output and effective processing.
- Default Standard, explicit fallback status, no implicit chaining. RTX is incomplete until real SDK processing runs.
- No automatic commit or release; user authorized implementation and routine reversible choices.

## Review Focus
- DPI/letterbox size must use physical video pixels (Task 3 test).
- Pending export must survive resize/mode changes without reading overwritten resources (Task 3 test).
- Flat colors/edges must not become NaN or show coefficient/index mistakes (Task 1 tests).
- SDK unavailable/failed must retain playback and report effective fallback (Task 2/3 tests).
- Old settings and invalid sharpness must not change default playback or panic (Task 1/3 tests).

### Task 1: Spatial GPU core and reference verification
Files: create src/upscale.rs, src/app/upscale_gpu.rs and vendor shader/reference files; modify src/lib.rs and src/app/color_gpu.rs; update THIRD_PARTY.
Interfaces: UpscaleMode {Standard,Nis,Fsr1,RtxVideo}; UpscaleSettings {mode,nis_sharpness,fsr_sharpness}, serde/default/Copy/PartialEq, normalize. Default Standard, sharpness0.3. Effective geometry requires both target dimensions exceed source. Preserve existing color_gpu::paint signature and add explicit enhanced entry point; record final interface in report. Offscreen render/readback must use same shader pipelines as playback and support async completion for Task3.
- [x] Fetch pinned official NIS/FSR sources and licenses. Add tests for settings, clamp and geometry, run to witness missing behavior.
- [x] Implement exact spatial algorithms in WGSL, persistent textures/pass chain, source-resolution grading before scale; native target dimensions.
- [x] Validate actual GPU pipeline, constant color, ramps, edge patterns and reference agreement; test resource reuse and bypass. Report actual API for next task.
- [x] Review task diff and fix substantive findings.

### Task 2: RTX SDK and native GPU backend
Files: native RTX module and minimal dependency/native build changes only after verified SDK contract.
- [ ] Obtain official SDK through public NVIDIA routes, read actual headers/samples/license. If all routes fail record exact blocker; do not fabricate bindings.
- [ ] Implement capability discovery, native DX12 lifetime/barrier/synchronization using same GPU device, actual VSR evaluation. No CPU roundtrip or frame accumulation.
- [ ] Verify unsupported/error fallback, actual evaluation output and performance; document redistributable runtime setup. Gate completed only with genuine SDK.
- [ ] Review task diff and fix substantive findings.

### Task 3: Settings, UI and export integration
Files: src/settings/mod.rs, src/app/{mod,presentation,color_page,color_integration,session}.rs and dedicated upscale UI/export module as needed.
- [x] Add tests for persisted defaults/normalization, effective mode, physical viewport and frozen export recipe including mode/output-size changes.
- [x] Persist UpscaleSettings. Add compact selector, supported sharpness controls, transient comparison, requested/effective resolution/status in Image page/overlay.
- [x] Route actual frame rect to GPU core, bypass at native/downscale; keep source upload ownership.
- [x] Snapshot source/settings/physical size; bound screenshot request to one, asynchronously render/readback same pipeline and background encode PNG. Original stays source-resolution. Handle errors without silent fallback mismatch.
- [x] Run matching PNG tests, screenshot-during-resize, legacy settings and narrow UI tests.

### Task 4: End-to-end acceptance
- [x] Strict fmt/Clippy, all tests and release build using existing portable GNU toolchain.
- [x] Benchmark1080p->1440p warmed modes with grading on/off; distinguish GPU/component timing from HDMI latency. Report RTX capability and any unresolved SDK gate honestly.
- [x] Fresh whole-change review, fix material findings; update README/validation docs and plan evidence. No publishing.

## Execution ledger
| Check | Finding/ruling |
|---|---|
| Task1 internal | GPU/settings references paired; exact vendor source required, no approximations under vendor labels. |
| Task2 internal | SDK inaccessible is a dependency gate, not permission to mark fake mode complete. |
| Task3 internal | Same pipeline export, frozen recipe; async readback only on export. |
| Task4 internal | Checks match requested feature; no physical HDMI claim from synthetic timing. |
| Task1/2 | Share GPU integration: Task2 attaches after Task1 API known; no simultaneous edits. |
| Task1/3 | Task1 report fixes enhanced paint/export API before Task3 modifies callers. |
| Task2/3 | Capability/status contract surfaced in UI; unknown RTX controls not invented. |
| Task3/4 | Full integration tests precede final review. |

Ruling: user explicitly said implement; proceed without another plan-approval question. Implementation workers sequential; read-only SDK investigation may overlap. No worktree needed for existing clean dedicated workspace with approved uncommitted design. Task commits omitted under design no-auto-commit constraint.

### Task2 dependency evidence
2026-11-11: official developer ZIP redirects to login HTML. Public NGC catalog signed download, both API namespace forms, file/zip/version endpoints returned HTTP403; alternate developer.download path404. No SDK headers/runtime acquired. Genuine RTX API/interop implementation cannot proceed until an official archive is supplied. Do not mark unavailable UI mode as completed RTX implementation. Spatial core/UI/export continue independently.

Baseline: cargo test --all-targets --locked passed 83 library +26 app tests; one timing test ignored as expected.

### Integration evidence
Settings persistence regression was observed failing (missing upscale field) then passing. Four app tests now cover physical DPI dimensions, exact egui viewport endpoint rounding/clipping, comparison and unavailable RTX fallback without settings mutation, and frozen export recipe across frame/mode/color/size changes. UI review found pending-export feedback and separate preview status gaps; both fixed and scoped re-review clean. Actual release UI snapshots main1200x800 and overlay960x600 were generated and visually checked; controls fit, content scrolls within bounded drawer.

Implementation scheduling adjustment: root prepared disjoint UI/settings wiring while the only implementation worker owned shaders/GPU files. No simultaneous implementation workers or overlapping file edits beyond the agreed module declaration. RTX remains an external dependency gate; user-supplied reference projects independently require the same SDK and do not include it.

Final status: Tasks1/3 spatial implementation and integration complete; Task4 available-mode verification complete (85+32 tests, strict Clippy, release build, UI snapshots, whole-change review clean). Task2 remains blocked on official SDK acquisition; full three-mode feature is not complete. No commit/release created.
