# Dictation

Marketplace app for Kosmos Desktop. It owns the Dictation UI and requests only the five manifest-scoped `dictation.*` operations; microphone, global hotkey, overlay lifecycle, credentials, transcription and text injection remain in the Cortex host.

```powershell
bun install --frozen-lockfile
bun run check
```

Install the resulting `release/dictation-<version>.kspkg` in Kosmos Marketplace or through Settings → Extensions.
