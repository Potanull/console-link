# Discord integration 0.4

## Scope and platform constraints

Use existing desktop IPC with the shared application ID, explicit linked-account gate and `Playing` activity. The public OAuth login confirms identity and supplies profile/avatar; it does not grant REST activity writes. Scope remains `identify`, PKCE S256 and OS credential store. No bot tokens, user tokens or partner RPC scopes.

Latest Discord documentation supports `activity.name` for the selected game, `status_display_type=2`, art, timestamps and up to two buttons. Use maintained discord-rich-presence serialization, bounded transport and an acknowledged SET_ACTIVITY request. Expose a rejected/overridden name rather than claiming success. The public API does not offer starting Go Live or forcing Discord's stream prompt. Provide a game-named window; the user selects a voice call/window and starts sharing in Discord. The application has no share popup or desktop Discord launch action.

## Data flow

```text
Capture frame -> bounded local OCR worker -> full Switch catalog -> repeated confident match
                                                               -> current game
OAuth identify -> verified account -> account-matched IPC -> name / cover / elapsed
               -> avatar artwork worker -> account card
Valid loopback callback -> tokenless application URI + completion event -> existing window focus
```

## Detection

Use ocrs 0.14 with rten 0.28. Native Vision/Windows.Media.Ocr was rejected for the first cross-platform implementation because Windows.Media.Ocr requires package identity absent from the installer/portable build. Two pinned, SHA256-checked models total 12,212,656 bytes; download when enabled and a capture frame exists, cache locally. Capture pixels never leave the machine. Model code and metadata have separate licenses, distributed notices include both.

This is dynamic image-to-title recognition against 24,017 base-game metadata entries, not user-supplied presets. Every four seconds analyze the latest available frame away from media/UI threads. Require repeated matching observations, reject unrelated competing titles and stale sessions. Manual correction takes priority until explicitly returning to automatic mode. Disabling stops new work and invalidates in-flight results.

OCR recognizes Latin text. It needs readable full names on HOME/title/menu screens. Arbitrary gameplay without a readable title and stylized logos cannot be reliably identified by this method. Keep the last confirmed title through ordinary gameplay and never invent a new match. The global catalog is a dated snapshot and does not guarantee every regional release.

## Return and presentation

Keep the registered loopback OAuth transport. A validated callback clears code/state from the browser URL and opens `console-link://auth/complete`; its only authority is window activation. Native completion also restores/focuses the viewport independently. Windows installer registers a per-user URI handler; its second process finds an existing window owned by the same executable and restores/focuses it. macOS registers CFBundleURLTypes with LaunchServices. A styled completion page retains an explicit return button when browser protocol launching is blocked. No credentials enter a custom URI.

Artwork downloads use allowed public HTTPS hosts, no redirects, bounded transfer/decode size and retry backoff. UI cards show real profile avatar, game art, platform, elapsed time, detection state and actual IPC result. No fake friends, party size, watch buttons or automatic broadcast claims.

## Sources

- https://docs.discord.com/developers/discord-social-sdk/development-guides/setting-rich-presence
- https://docs.discord.com/developers/topics/rpc#set-activity
- https://docs.discord.com/developers/events/gateway-events#activity-object
- https://docs.discord.com/developers/discord-social-sdk/development-guides/account-linking-with-discord
- https://docs.discord.com/developers/resources/user#user-object
- https://support.discord.com/hc/en-us/articles/360040816151-Go-Live-and-Screen-Share
- https://github.com/robertknight/ocrs
- https://huggingface.co/robertknight/ocrs
- https://github.com/blawar/titledb
- https://learn.microsoft.com/en-us/uwp/api/windows.media.ocr
