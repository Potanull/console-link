# Local validation of 0.7.3

2026-11-18, based on release v0.7.2 (`c883515`). This is local validation, not
evidence of a published release or Windows runtime acceptance.

- Full macOS checks passed: strict Clippy, 131 library tests, 12 desktop tests,
  and 6 Nintendo helper tests. Six opt-in library tests were excluded by default.
- The additional opt-in power test passed: `pmset -g assertions` showed this
  process's `PreventUserIdleDisplaySleep` and `PreventUserIdleSystemSleep`
  assertions during an accepted capture session, and neither after Runtime drop.
  Lifecycle tests also covered stop, disconnection, close, failed startup and
  cancellation. The capture session in these tests is a test double.
- Windows GNU library type checking passed with the new `Win32_System_Power`
  feature. This does not replace native Windows execution.
- Optimized macOS preview built in 82 seconds. The packaged app passed the
  native synthetic smoke: 612 presented frames, windowed/borderless/fullscreen
  transitions, and no playback GPU readback.
- PNG, all seven ICNS entries and all six ICO entries were independently decoded.
  Corners are transparent. macOS `sips` also decoded the packaged ICNS successfully.
- Bundle version is 0.7.3, build 12. Plist validation and strict ad-hoc signature
  verification passed. The installed app's icon matches the new source asset.
- Removed the on-video F1 button; the F1 binding and drawer handler remain.

Still unverified: a real multi-hour capture session, native Windows power requests,
and Finder/Dock rendering after an upgrade on other machines. No notarization or
Windows code signing was added. The user's system power preferences are unchanged.
