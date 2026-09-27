# KOS-193 reproduction artifacts (second pass — prepended)

Independent reproductions captured BEFORE the fixes on `kos-193`. Base
included all merged kos-172 fixes; these are NEW bugs.

## W1 — Stale `finishCapture` continuation (`worker/worker.ts`)

`worker-cancel-transcribe-prefix.txt` — two vitest reproductions, both fail
pre-fix:

1. Cancel while `speech.transcribe` is in flight → re-trigger a new session →
   the stale continuation then emits `dictation.input.insert_text` with
   session-1's text aimed at **session-2's** `targetWindow` ("w2"), then wipes
   session-2's `captureId`/`windowId` and forces state idle while the Engine
   still records capture-2 (cross-session contamination + bricked session).
2. Same setup without re-trigger → the stale continuation throws
   `window-id-missing` → a `dictation.error` event + `ok:false` invoke reply
   for a cancel the USER asked for.

Fix: after every `await` inside `finishCapture`, bail when
`this.captureId !== captureId` — cancel() cleared it, or a newer session owns
the slot.

## G1 — `dictation_ptt_trigger` ignores `phase` (`gpui/src/app.rs`)

`gpui-ptt-sim.rs` + `gpui-ptt-sim.txt` — standalone rustc simulation of the
routing (the crate can't build here: deps require rustc 1.87, env has 1.85;
see kos-172's note about the shader-artifact build-script failure too).
Both PTT phases were routed into `dictation_toggle`. Shown in the sim:

- bare `up` while idle → starts capture (release after pill Отмена, or key
  held at app launch, immediately re-arms/arms recording),
- `up` during Starting → dropped → capture lands in Recording with the key
  released — records forever,
- repeat `down` while Recording → premature finish,
- `up` while Processing → preempts into a brand-new capture.

Fix: phase-aware routing + `finish_after_start` (worker.ts `stopAfterStart`
parity).

## G2 — `hotkey_capturing` stuck on failed arm (`gpui/src/app.rs`)

Same sim file, second section: `dictation.begin_hotkey_capture` was sent via
the anonymous "@action" slot; on Err the flag stayed true and only the
`dictation_capture_key`/`_cancelled` WS events could clear it — they never
arrive after a failed arm, so the status window shows "Нажмите комбинацию…"
until restart. Fix: dedicated `dictation.hotkey_capture` slot whose Err reply
clears the flag; dead-channel send clears it immediately.

## V1 — Save after failed load overwrites Engine config (`src/App.vue`)

`settings-save-prefix.txt` — house-style source-assertion test failing
pre-fix. On `get_config` failure the form keeps hardcoded fallbacks and
`save()` would POST them over the user's real config (incl.
`autostart:false`). Fix: `settingsLoaded` gate — save disabled + refused
until a config actually landed.

---

# KOS-172 reproduction artifacts

Independent reproductions captured BEFORE the fixes on `kos-172`.

## Worker bugs (`worker/worker.ts`)

`worker-prefix-failures.txt` — `bun run test -- tests/workerProtocol.test.ts`
run against the PRE-FIX worker (the fix was stashed). 6 failures:

1. `heartbeats so the supervisor does not reap a running worker`
   `expected 0 to be greater than 0` — 65 s of virtual time produced zero
   `worker.heartbeat` messages. The Cortex supervisor
   (`heartbeat_watch`, 60 s deadline, 30 s tick) fails a Running worker that
   stays silent, so the dictation worker was reaped ~1 min after `worker.hello`.
2. `ignores a second trigger while a capture is still starting`
   `expected [ ...(2) ] to have a length of 1 but got 2` — a second
   `dictation.trigger` arriving while `window.foreground` / `capture.start`
   were in flight launched a second start sequence (state was still `"idle"`),
   producing two `window.foreground` calls. The second foreground result
   overwrites the contract window id, so the first session's `insert_text`
   targets a stale window, and the Engine can report `busy`.
3. `finishes the capture when push-to-talk is released mid-start`
   — a `ptt` `up` during the start sequence was dropped: no
   `dictation.capture.stop` / `dictation.speech.transcribe` ever issued, so the
   microphone keeps recording after the user released the hotkey.
4. `stops a live Engine capture when cancelled mid-capture` — `cancel()`
   dropped `captureId` without telling the Engine; the live session stayed
   busy forever. Reachable via `dictation.cancel` and via the stdin pump on
   malformed lines.
5. `stops the Engine capture that resolves after a mid-start cancel` — a
   `capture.start` resolving after `cancel()` was adopted as the live
   session; the orphaned Engine capture was never stopped.
6. `ignores an up-phase toggle trigger instead of double-toggling` —
   `{kind:"toggle", phase:"up"}` while idle started a capture (test timed out
   waiting on the stray call) — an up-phase must never start a recording.

Fixes: `"starting"` state + `startSeq` epoch guard, `stopAfterStart` deferred
finish, `cancel()` issues `dictation.capture.stop` for a live id, up-phase
gated to `ptt`, empty `audioB64` finishes silently instead of flashing
`audio-missing`, and a 15 s heartbeat (`generation`+`token`, `.unref()`d).

`postfix-tests-green.txt` — suite after the fix.

## GPUI single-instance (`gpui/src/main.rs`)

`claim_single_instance` sampled `GetLastError()` AFTER `CreateEventW`. The
Win32 contract requires reading it immediately after the call being checked
(`CreateMutexW`); `CreateEventW` overwrites the last-error value. Consequence:
a stale `ERROR_ALREADY_EXISTS` exits the FIRST instance, and an event-create
failure path can launch a duplicate instance. Fix clears last-error
(`SetLastError(0)`) before `CreateMutexW` and captures the result before
`CreateEventW` — the MSDN-documented pattern.

No automated Windows test exists for this path (needs real Win32 semantics).
`gpui-wincheck.txt` — the exact changed function, extracted verbatim into a
minimal `windows-sys` crate, type-checks clean for
`x86_64-pc-windows-msvc` (dead-code warnings only). Full `cargo check` on the
gpui crate is NOT_RUN: the `gpui-pre-windows` build script requires a shader
compiler artifact (`out/shaders_bytes.rs`) that is not produced in this Linux
environment — failure is pre-existing and unrelated to the change.
