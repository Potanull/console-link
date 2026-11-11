# Overlay and playback correction

User requested correction of the game overlay, investigation/fix of stuttering and removal of the share-game popup.

- Bound the drawer to the viewport, fix its header/footer, disable dragging and isolate scroll positions by tab.
- Use compact audio/EQ layout in gameplay; retain full preset editing in a disclosure and in the main audio page.
- Remove share prompt state, settings, rendering and constant. Ignore its old persisted flag through serde compatibility.
- Avoid serializing settings on every gameplay frame; compare typed settings only while the drawer is rendered.
- Measured old debug 1080p BGRA conversion at 16.10 ms/frame versus 1.57 ms optimized. Optimize the application in dev and produce a release build. This does not prove the complete hardware path meets 60 FPS.
- Regression: drawer fits 480×270 through 1920×1080; run full suite, format check, snapshots of short/tall overlays and game/menu cycle. Do not interrupt the user's active capture session.

User clarification: all ten sliders must always stay on one line in both main audio page and overlay; implemented with ten columns and responsive spacing.

The first rectangle-only regression was insufficient: independent review identified frame border thickness and footer allocation as extra overflow. A rendered egui regression reproduced the failure and now passes. Content size subtracts `Frame::total_margin`; footer reserves actual42px action height,10px gap and item spacing. New egui text-shape regression verifies all ten frequency labels share one y-coordinate in narrow/main layouts. Legacy `show_share_prompt` is ignored without losing preferences. Full suite:74 library +20 application tests passed.

Measured standalone current-source timings on this machine: debug BGRA1080p16.10ms/frame, optimized1.57ms/frame; ten seconds EQ input debug93.98ms versus optimized32.78ms. These are component timings, not a full capture-to-display latency measurement. No active capture process was stopped.
