# Dictation: agent instructions

## Scope and entry points

Marketplace Dictation UI packaged for Cortex.

> Status (2026-09-24, repo-split decision): the Dictation UI is moving INTO
> Cortex as a GPUI module of the unified application (manager-gpui). This
> packaged Vue UI is being phased out — treat the repo as frozen for new
> feature work until the transition completes.

- `src/`: renderer UI and scoped host-operation requests.
- `manifest.json`, `package.manifest.json`, `compatibility.json`: identity, six operations and migration contract.
- `tests/`, `scripts/`: Vitest checks, manifest validation and package smoke.

Read README, current package scripts, relevant tests and any nested AGENTS.md first.
Check current git status and task/PR revision; preserve unrelated changes.
Use current implementation as evidence, not old issue descriptions.

## Setup and verification

Run from this repository root. Prefix shell commands with `rtk`; use `rtk proxy`
when unfiltered output is needed. Do not bypass hooks to obtain a green result.

```powershell
rtk bun install --frozen-lockfile
rtk bun run check
```

- Use Bun 1.3.14 from `packageManager`. Full check includes tests, build and `.kspkg` smoke.
- Output: `release/dictation-<version>.kspkg`. Keep package versions synchronized through existing validation.

## Contracts to preserve

- Request only the six manifest-scoped `dictation.*` operations.
- Microphone, hotkeys, overlay lifecycle, credentials, transcription and text injection remain in Cortex; do not implement renderer bypasses.
- Preserve legacy Dictation identity, permissions, credentials, settings and speech assets through bounded 0.x compatibility.
- App mocks do not prove native microphone/hotkey/permission acceptance. Hand off exact package SHA and required native scenarios to the Cortex owner.

- Preserve existing UI language; reuse Imago tokens/components and preserve keyboard navigation, focus behavior and accessible names.
- Clean up listeners, timers and subscriptions on disposal. Verify visual changes in the running UI, or report UI verification NOT_RUN with its reason.

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
- For code, manifest or dependency changes, run relevant checks during work and the full gate on the integrated revision. Documentation-only edits need path/command and diff checks, not an application rebuild.
- For nontrivial work, use an independent Luna review of the integrated revision.
- Report exact SHA (and dirty diff if applicable), dependency revisions, commands and PASS / FAIL / NOT_RUN with reasons. Never claim a missing native or external check passed.
- Follow the current organization quality contract; CI absence is not evidence of failure or success, and local checks do not bypass protected-branch rules.
- Do not publish releases, upload artifacts, alter access or close umbrella issues unless that action is authorized. Never replace bytes of an already published version.

## Worktree bootstrap

- Before delegating, ensure this AGENTS.md exists in the new worktree. Git does not carry uncommitted instructions into worktrees: copy only the approved instruction file if absent; do not copy unrelated working changes.
