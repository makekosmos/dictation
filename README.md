# Dictation

Standalone GPUI dictation app for Kosmos Engine (`gpui/`, crate
`dictation-gpui`). It owns the status window, the pill overlay, hotkey capture
and the settings UI, and talks to the Engine directly over RPC/WS
(`dictation.capture.*`, `dictation.speech.transcribe`, `dictation.cancel`, the
`dictation.*` config/state operations). Microphone, global hotkey,
transcription providers and text injection remain Engine-owned (cortex
`runtime/src/dictation/`); the app never accesses them directly.

```powershell
cargo fmt --manifest-path gpui/Cargo.toml -- --check
cargo clippy --locked --manifest-path gpui/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --locked --manifest-path gpui/Cargo.toml --all-features
python scripts/test_release.py
```

`build.yml` builds the app on Windows, Linux and macOS; scheduled runs publish
`gpui-vX.Y.Z` GitHub releases with the packaged binaries. The legacy Vue
`.kspkg` package (`v0.2.x` and earlier) is retired; its published releases stay
on GitHub untouched.
