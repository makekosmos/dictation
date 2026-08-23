# Dictation

Marketplace extension for Kosmos Desktop. It owns the Dictation UI and requests only `dictation.control`; microphone, global hotkey, overlay lifecycle, credentials and text injection remain in the Desktop host.

```powershell
$env:NODE_AUTH_TOKEN = gh auth token
bun install
bun run build
bun run package:kext
```

Install the resulting `.kext` in Kosmos Marketplace or through Settings → Extensions.
