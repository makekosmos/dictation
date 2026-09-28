# Dictation: agent instructions

## Scope and entry points

Standalone GPUI dictation app for Mundus Engine. The legacy Vue `.kspkg`
package was retired (removed in this change); only the Rust crate ships.

- `gpui/`: the `dictation-gpui` crate — status window, pill overlay, hotkey
  capture, settings UI. `gpui/src/worker.rs` holds the blocking Engine calls
  on a worker thread; `gpui/src/app.rs` owns the state machine.
- `scripts/release.py`, `scripts/publish-version.sh`,
  `scripts/test_release.py`: `gpui-vX.Y.Z` versioning, packaging and
  publication used by `.github/workflows/build.yml`.
- `artifacts/`: historical reproduction notes; reference only, not a gate.

Read README, relevant `gpui/src` code and any nested AGENTS.md first.
Check current git status and task/PR revision; preserve unrelated changes.
Use current implementation as evidence, not old issue descriptions.

## Setup and verification

Run from this repository root. Prefix shell commands with `rtk`; use `rtk proxy`
when unfiltered output is needed. Do not bypass hooks to obtain a green result.

```powershell
rtk cargo fmt --manifest-path gpui/Cargo.toml -- --check
rtk cargo clippy --locked --manifest-path gpui/Cargo.toml --all-targets --all-features -- -D warnings
rtk cargo test --locked --manifest-path gpui/Cargo.toml --all-features
rtk python scripts/test_release.py
```

- Git hooks: `hk.pkl` (hk, same tool as agenda-gpui) — `hk install`; pre-commit
  runs fmt + check, pre-push/`hk check` runs clippy, tests and the release-rule
  checks.
- The clippy warning baseline (`-A` list) must stay identical to `build.yml`'s
  Windows clippy step and agenda-gpui.
- Version source of truth: `gpui/Cargo.toml`. Release tags are `gpui-vX.Y.Z`;
  the legacy kspkg `vX.Y.Z` line belongs to already-published releases and is
  never reused or modified.

## Contracts to preserve

- The app reaches the Engine only through `mundus-gpui-kit`'s `Engine` RPC/WS
  client (`dictation.capture.*`, `dictation.speech.transcribe`,
  `dictation.cancel`, `dictation.*` config/state ops). Microphone, hotkeys,
  overlay injection, credentials, transcription providers and text insertion
  stay in cortex `runtime/src/dictation/`; do not implement app-side bypasses.
- cortex's Windows installer build pins this repo
  (`desktop/component-pins.json`, `dictation_gpui`) and builds the crate from a
  sibling checkout; coordinate layout changes with the cortex owner.
- Preserve existing UI language (Russian strings); reuse Imago/mundus-gpui-kit
  tokens/components and preserve keyboard navigation, focus behavior and
  accessible names.
- Clean up listeners, timers and subscriptions on disposal. Verify visual
  changes in the running UI, or report UI verification NOT_RUN with its reason.

## Parallel work

- This chat owns only its assigned repositories; sibling repositories are read-only unless explicitly assigned.
- Each writer uses a separate worktree. Never switch branches, reset, clean or stash another writer's checkout.
- The lead may use a few Luna subagents for bounded independent work. Give each an acceptance condition, file boundary and dependency revision.
- Assign manifests, lockfiles, shared helpers and generated outputs to one writer. Integrate returned commits sequentially.
- Pin external dependencies by version/SHA; if existing tooling needs sibling paths, provision isolated pinned checkouts instead of changing another chat's code.
- Hand off contract changes with operation/type, inputs, outputs, errors, version and compatibility evidence. Finish independent work while a dependency is pending.

## Completion

- Prefer existing code and tools; avoid unrelated cleanup and new abstractions. Trace callers before fixing a shared bug.
- Add the smallest meaningful regression check for changed nontrivial behavior; include failure paths for permissions, migrations or persistence.
- For code or dependency changes, run relevant checks during work and the full gate on the integrated revision. Documentation-only edits need path/command and diff checks, not an application rebuild.
- For nontrivial work, use an independent Luna review of the integrated revision.
- Report exact SHA (and dirty diff if applicable), dependency revisions, commands and PASS / FAIL / NOT_RUN with reasons. Never claim a missing native or external check passed.
- Follow the current organization quality contract; CI absence is not evidence of failure or success, and local checks do not bypass protected-branch rules.
- Do not publish releases, upload artifacts, alter access or close umbrella issues unless that action is authorized. Never replace bytes of an already published version.

## Worktree bootstrap

- Before delegating, ensure this AGENTS.md exists in the new worktree. Git does not carry uncommitted instructions into worktrees: copy only the approved instruction file if absent; do not copy unrelated working changes.
