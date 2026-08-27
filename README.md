# Dictation

Marketplace app for Kosmos Desktop. It owns the Dictation UI and requests only the six manifest-scoped `dictation.*` operations; microphone, global hotkey, overlay lifecycle, credentials, transcription and text injection remain in the Cortex host.

```powershell
bun install --frozen-lockfile
bun run check
```

Install the resulting `release/dictation-<version>.kspkg` in Kosmos Marketplace or through Settings → Extensions.

`compatibility.json` keeps the legacy `dictation` extension identity, permissions,
credentials, settings, and local speech assets available during the migration;
the compatibility layer is bounded to the 0.x line and can be removed after 1.0.0.
