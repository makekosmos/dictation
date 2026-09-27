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
