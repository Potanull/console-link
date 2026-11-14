# Desktop interface

Console Link uses a quiet monochrome interface built in Rust and GPUI 0.2.2.
The agreed direction is functional desktop software: clear labels, rounded controls,
consistent navigation and minimal decoration. It does not copy Discord colors.

## Structure

- Connection: one central card for video, audio input/output and the primary connection action.
- Navigation: the same five icons with text on every page, including the gameplay drawer.
- Settings: one flexible column, at most 920 logical pixels wide. Connection uses at most 820.
- During play: video fills the window; F1 opens the right drawer, Escape closes it.
- Drawer: volume, mute and window mode come first; detailed controls use the same pages.
- Presets and device choices use shared floating menus, never a wall of preset buttons.
- The logo is a solid gamepad with the Triforce integrated into its right controls.
  The wordmark is separate and remains selectable interface text.

## Tokens and controls

`src/gpui/theme.rs` owns semantic monochrome colors. Spacing follows 4/8-pixel steps;
body controls use Inter at 14px, secondary labels 13px, section titles 17px.
Buttons, switches and selectors have a minimum height of 40px.
Focus, selection and disabled/loading states must remain distinguishable.

Measured WCAG contrast for text roles on background/surface/elevated/hover surfaces:
primary 12.11-16.90:1, secondary 6.65-9.29:1, muted 4.63-6.47:1.
These are calculated token pairs, not a claim of complete native accessibility support.

## Skill sources

Installed and applied from the exact user-requested SkillFoxx listings:

- [UI UX Pro Max](https://skillfoxx.ru/skills/ui-ux-pro-max), upstream
  `nextlevelbuilder/ui-ux-pro-max-skill` at `6ae86f61f520d6ca229d2f40e070410638c908c5`.
- [Color Expert](https://skillfoxx.ru/skills/color-expert), by meodai, CC BY 4.0,
  `meodai/skill.color-expert` at `5bf7c7704d56bccc6aeea38c36bcf449abe56a6c`.
- [Interfaces skills](https://skillfoxx.ru/skills/interfaces-skills), by Jakub Krehel,
  `jakubkrehel/skills` at `a72290d140249813898ba8648becf6415abcb059`.

Applied principles: explicit layout constraints, typography hierarchy, semantic color roles,
measured contrast, shared control styling and review based on observable defects.
Web-specific APIs and mobile breakpoints are not copied into native GPUI.
The user's dark monochrome direction overrides generic palette suggestions.

## Visual verification

Native preview commands (no persisted account/settings changes):

```sh
cargo run -- --isolated --ui-size 1280x840
cargo run -- --isolated --ui-size 760x600 --ui-page discord
cargo run -- --isolated --ui-page audio --ui-select audio
cargo run -- --isolated --ui-page image --ui-select color
cargo run -- --synthetic --ui-page audio --smoke-seconds 20
```

Inspect all five pages, open selectors, long values, smallest window and gameplay drawer.
A successful build or GPU smoke does not establish visual correctness. Native screenshots
require macOS screen-recording permission; user-provided screenshots are valid evidence.
