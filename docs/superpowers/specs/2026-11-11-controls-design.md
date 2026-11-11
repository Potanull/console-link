# Console Link controls

Approved in chat: menu starts at 1200 × 800 with responsive cards. Add a ten-band equalizer with Neutral, Bass, Speech, Night and Gaming factory presets and saved, editable user presets. Remove screen recognition, its worker, model downloads and OCR dependencies. Keep manual game selection, covers, account authentication and Discord presence. Remove all Discord launch controls. Add real FPS/audio histories, dropped-frame and underrun counters, capture parameters and session duration in About.

Use existing Rust/egui/CPAL architecture. Process PCM directly: adding a codec would introduce unnecessary encoding and delay. Keep screenshot saving as a user action. Keep schema 1 compatibility by defaulting new fields and ignoring obsolete fields. Factory presets are immutable; users save copies. Limit gains to ±12 dB and trim preamp to reserve headroom. Graphs show an empty state when there is no session.

Approaches: native PCM biquads avoid extra runtime dependencies and encoding; an external audio effects framework would add integration and packaging overhead without a needed capability. Use native PCM biquads.
