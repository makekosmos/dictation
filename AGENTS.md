# Dictation: agent instructions

## Scope and entry points

Kosmos Dictation — standalone GPUI app (`dictation-gpui`), packaged by Cortex
as a Kosmos component (`resources/components/dictation/Kosmos Dictation.exe`).

> Status (2026-09-28, KOS-241): the Vue UI + Kosmos Marketplace package
> (`kosmos-host` target, `.kspkg`) and the standalone worker
> (`worker/dictation-worker.exe`) have been removed. `dictation-gpui` is the
> product; it talks to Kosmos Engine's `dictation.v2` RPC/WS surface directly
> (no worker subprocess, no marketplace host). Dictation settings and hotkey
> configuration live in GPUI Manager; this repo owns only the pill overlay,
> window/autostart lifecycle and the Engine client.

- `src/`: the GPUI app (pill overlay, window, autostart, background mode,
  Engine RPC/WS client).
- `build.rs`, `windows/`: Windows icon + VERSIONINFO stamping for the
  Cortex-packaged exe (`KOSMOS_DICTATION_VERSION`).
- `compatibility.json`: legacy `dictation` / `com.kosmos.dictation` identity
  and data-root contract, preserved for user-data continuity.

Read README, current source and any nested AGENTS.md first.
Check current git status and task/PR revision; preserve unrelated changes.
Use current implementation as evidence, not old issue descriptions.

## Setup and verification

Run from this repository root.

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Cargo resolves through the shared `mbx` build cache automatically on this
machine; no separate bootstrap step is required. A release build for the
Cortex packaging shape:

```powershell
$env:KOSMOS_DICTATION_VERSION = "<version>"
cargo build --release --target x86_64-pc-windows-msvc
```

## Contracts to preserve

- Microphone, hotkeys, overlay lifecycle, credentials, transcription and text
  injection remain in Kosmos Engine; this app only calls the manifest-scoped
  `dictation.*` Engine RPC operations and listens to its broadcast WS events.
- Preserve legacy Dictation identity, permissions, credentials, settings and
  speech assets through `compatibility.json` — nothing about where user data
  lives changes with this repo's packaging format.
- App mocks do not prove native microphone/hotkey/permission acceptance. Hand
  off exact commit SHA and required native scenarios to the Cortex owner.

- Preserve existing UI language (Russian); reuse Imago/kosmos-gpui-kit
  tokens/components and preserve keyboard navigation, focus behavior and
  accessible names.
- Clean up listeners, timers and subscriptions on disposal. Verify visual
  changes in the running app, or report UI verification NOT_RUN with its
  reason.

## Parallel work

- This chat owns only its assigned repositories; sibling repositories are
  read-only unless explicitly assigned.
- Each writer uses a separate worktree. Never switch branches, reset, clean or
  stash another writer's checkout.
- The lead may use a few Luna subagents for bounded independent work. Give
  each an acceptance condition, file boundary and dependency revision.
- Assign manifests, lockfiles, shared helpers and generated outputs to one
  writer. Integrate returned commits sequentially.
- Pin external dependencies by version/SHA; if existing tooling needs sibling
  paths, provision isolated pinned checkouts instead of changing another
  chat's code.
- Hand off contract changes with operation/type, inputs, outputs, errors,
  version and compatibility evidence. Finish independent work while a
  dependency is pending.

## Completion

- Prefer existing code and tools; avoid unrelated cleanup and new
  abstractions. Trace callers before fixing a shared bug.
- Add the smallest meaningful regression check for changed nontrivial
  behavior; include failure paths for permissions, migrations or persistence.
- For code or dependency changes, run relevant checks during work and the
  full gate on the integrated revision. Documentation-only edits need
  path/command and diff checks, not an application rebuild.
- For nontrivial work, use an independent Luna review of the integrated
  revision.
- Report exact SHA (and dirty diff if applicable), dependency revisions,
  commands and PASS / FAIL / NOT_RUN with reasons. Never claim a missing
  native or external check passed.
- Follow the current organization quality contract; CI absence is not
  evidence of failure or success, and local checks do not bypass
  protected-branch rules.
- Do not publish releases, upload artifacts, alter access or close umbrella
  issues unless that action is authorized. Never replace bytes of an already
  published version.

## Worktree bootstrap

- Before delegating, ensure this AGENTS.md exists in the new worktree. Git
  does not carry uncommitted instructions into worktrees: copy only the
  approved instruction file if absent; do not copy unrelated working changes.
