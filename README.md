# Dictation

Marketplace app for Kosmos Desktop. It owns the Dictation UI and a Windows worker that orchestrates capture, transcription and insertion through the manifest-scoped `dictation.v2` Engine operations. Microphone, global hotkey, overlay lifecycle, credentials, transcription providers and text injection remain Engine-owned capabilities; the app never accesses them directly.

```powershell
bun install --frozen-lockfile
bun run check
```

Install the resulting `release/dictation-<version>.kspkg` in Kosmos Marketplace or through Settings → Extensions.

`compatibility.json` keeps the legacy `dictation` extension identity, permissions,
credentials, settings, and local speech assets available during the migration;
the compatibility layer is bounded to the 0.x line and can be removed after 1.0.0.
