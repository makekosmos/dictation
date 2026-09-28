# KOS-256 reproduction artifacts (fifth pass — prepended)

Independent reproductions captured BEFORE the fixes on `kos-256`. Base
included all merged kos-172 + kos-193 + kos-220 + kos-237 + kos-247 fixes;
these are NEW bugs. `cargo test`/`clippy`/`fmt` run on the ~/.rustup 1.95.0
toolchain (`RUST_FONTCONFIG_DLOPEN=1`), so the new `#[test]`s in app.rs and
view.rs are real gate coverage, not just sims.

## N1 — Reply mutations that never repaint (`gpui/src/app.rs`)

`gpui-notify-sim.rs` + `.txt` — `gpui::Context::notify()` is the ONLY
invalidation channel: `Entity::update` does not auto-notify observers
(verified against the gpui-kit 0.6.2 source). `handle_reply` mutated
`DictationApp` in several arms — notably the "@action" Err arm
(`self.error = …`) and `fail_pill` (phase→Processing, delivery→Failed) —
without a single `cx.notify()`. A failed click in the status window wrote
the banner text but repainted nothing until an unrelated event arrived;
a pill failure kept painting the old frame until the next level tick.
Fix: one final `cx.notify()` at the end of `handle_reply` (idempotent —
explicit notifies earlier in the match remain).

## D1 — Recording length rendered as a relative time (`gpui/src/view.rs`)

`gpui-duration-sim.rs` + `.txt` — the "Последняя расшифровка" card passed
`durationMs` to the kit's `fmt_ms`, which formats elapsed TIMESTAMPS as
relative phrases: a 90 s take printed "запись 2 мин. назад", a 1 h take
"запись 1 ч. назад". Fix: `fmt_duration()` formats clock length — `m:ss`
(`h:mm:ss` past an hour). Regression: `view::tests::duration_formats_as_clock`.

## R1 — Straggler level event re-adopts a just-ended capture (`gpui/src/app.rs`)

`gpui-readopt-sim.rs` + `.txt` — `capture.stop`/`dictation.cancel` reach
Engine asynchronously on the worker channel, but the WS events channel can
still hold `dictation_audio_level` frames produced before the stop landed.
The adoption arm (idle phase + captureId present → open pill as Recording)
accepted ANY id, so a drained straggler for the capture WE just ended
popped the pill back open on "Идёт запись" — and a trigger press in that
window called `dictation_finish` on an id Engine already dropped → bogus
"Не доставлено" for a user who pressed Отмена. Fix: `ended_capture`
tombstone set on every path that ends a session (cancel, finish, orphan
stop, start-reply-after-close, `fail_pill`, adopted-session retirement);
`should_adopt_capture` rejects the tombstoned id and empty ids while still
adopting foreign captures. Regression: `app::tests::adoption_skips_ended_capture`.

## S1 — "capturing" state renders green "Готов" + empty hotkey chips (`gpui/src/view.rs`)

`gpui-state-label-sim.rs` + `.txt` — two presentation defects, same view:

1. Engine's dotted state contract spells the live state `"capturing"`
   (`state_broadcast_ended` in app.rs already treats both spellings), but
   `state_label` and the badge-color match only knew `"recording"` — a live
   capture showed the green "Готов" (ready) badge. Fix: both matches take
   `"recording" | "capturing"`.
2. The hotkey row did `split('+')` without dropping empty parts, so a
   stored/legacy `"Ctrl++"` (produced before the KOS-247 `Plus` mapping)
   rendered `["Ctrl", "", ""]` — two ghost chips. Fix: trim + filter empty
   (pill.rs footer parity).

Regression: `view::tests::capturing_is_recording`.

---

# KOS-247 reproduction artifacts (fourth pass — prepended)

Independent reproductions captured BEFORE the fixes on `kos-247`. Base
included all merged kos-172 + kos-193 + kos-220 + kos-237 fixes; these are
NEW bugs. `cargo check`/`test`/`clippy` now run on Linux with the
~/.rustup 1.95.0 toolchain (`RUST_FONTCONFIG_DLOPEN=1`), so the new
`#[test]`s in app.rs are real gate coverage, not just sims.

## A1 — `@action` success leaves the stale error banner (`gpui/src/app.rs`)

`gpui-action-error-sim.rs` + `.txt` — logic sim of `handle_reply`'s "@action"
arm. `self.error` is documented "banner until a retry succeeds" and the
`dictation.pill.result` Ok arm already clears it, but the "@action" Ok arm
only set `notice` — after e.g. a failed `dictation.update_config` the banner
stayed up next to "Выполнено." forever. Fix: `self.error = None` on success.

## A2 — `Ctrl++` accelerator loses its key (`gpui/src/app.rs`, `useDictationConfig.shared.ts`)

`gpui-accelerator-sim.rs` + `.txt` — VK_OEM_PLUS (0xBB) mapped to "+", so
Ctrl+= captured "Ctrl++": '+' is the accelerator delimiter, the key part is
empty, and the pill footer's own `split('+')` shows only "Ctrl". Engine-side
registration of "Ctrl++" cannot express the key. Fix: `0xBB → "Plus"`
(Electron's name for the key), both ports. Regression: `cargo test`
`accelerator_names_oem_plus` + `accelerator_rejects_unmapped_vk`, and
vitest `tests/hotkeyAccelerator.test.ts` for the shared port.

## A3 — `list_local_models` failure flagged the whole load failed (`src/App.vue`)

`load-models-prefix.txt` — the settings-load regression check (a
`settingsUi.test.ts` source assertion, house style) evaluated against the
pre/post App.vue: `listLocalModels()` shared get_config's try, so a
missing/failed models op printed "Не удалось загрузить настройки" over a
form that had actually loaded — and every mount on an Engine without the op
showed a permanent bogus error. Fix: the models fetch is isolated in its own
try/catch; on failure the dropdown simply shows no local options.

## A4 — Cancelled transcribe reply clobbers the last transcript card (`gpui/src/app.rs`)

`gpui-cancelled-result-sim.rs` + `.txt` — logic sim of the
`dictation.pill.result` arm. On `cancelled: true` the pill closed correctly
but the stub was still written to `slots["dictation.result"]`, replacing the
status window's real "Последняя расшифровка" card with an empty entry.
Fix: the slot write now happens only for non-cancelled results.

---

# KOS-220 reproduction artifacts (third pass — prepended)

Independent reproductions captured BEFORE the fixes on `kos-220`. Base
included all merged kos-172 + kos-193 fixes; these are NEW bugs.

## G3 — `dictation-gpui` does not compile (`gpui/src/app.rs`)

`gpui-compile-prefix.txt` — `cargo +1.95.0 check` (toolchain in ~/.rustup,
deps resolve offline, `RUST_FONTCONFIG_DLOPEN=1` needed since pkg-config is
absent) fails on the merged kos-193 tree:

    error[E0308]: mismatched types
       --> src/app.rs:262:9
        match self.capture.take() { ... }   // arms yield bool
        expected `()`, found `bool`

KOS-193 made `send_command` return `bool` but left this `match` in statement
position. `bun run check` never builds the crate and the kos-193 gate used a
`wincheck` shim, so the break slipped through. Fix: `()`-typed match arms.

## G4 — Stuck pill on a dead worker channel (`gpui/src/app.rs`)

`send_command` returns false when the worker thread is gone, but
`dictation_begin`, `dictation_finish` and the `pill.stop`→transcribe handoff
ignored it — the pill would sit on "Запуск записи…"/"Распознаю" forever
(kos-193 fixed this class only for `hotkey_capture_start`). Simulated via
`gpui-empty-audio-sim.rs` (same harness, third section). Fix: `fail_pill` on
`!sent` at all three sites.

## G5 — Zero-length capture shows a bogus error pill (`gpui/src/app.rs`)

`gpui-empty-audio-sim.rs` + `gpui-empty-audio-sim.txt` — standalone sim of
the `dictation.pill.stop` reply arm, pre/post. `unwrap_or_default()`
collapsed missing and EMPTY `audioB64` into "" → `dictation_transcribe`
rejected it ("Engine не вернул аудио записи") → `fail_pill` shows
"Не доставлено" 4.5 s + sticky banner — for a plain quick PTT tap.
`worker.ts` `finishCapture` treats `""` as silent finish and only a MISSING
field as `audio-missing`. Fix: `Some("")` closes silently, `None` keeps the
error, non-empty audio transcribes as before.

## W3 — A stale session's failure cancels the live session (`worker/worker.ts`)

`worker-stale-failure-prefix.txt` — two vitest reproductions, both fail
pre-fix:

1. finish → `speech.transcribe` in flight → cancel → re-trigger → the stale
   transcribe's late REJECTION reaches `message()`'s catch → `cancel()`
   stops capture-2 (live!) and emits a bogus `dictation.error`. KOS-193
   guarded the success path (`captureId !== captureId`); the error path was
   unguarded.
2. start → `capture.start` in flight → cancel → re-trigger → the stale
   start's REJECTION likewise cancels session 2; a stale start with an
   id-less reply also threw `capture-id-missing` → same clobber.

Fix: `startCapture`/`finishCapture` catch blocks return quietly when the
session is no longer current (`startSeq` / `captureId` guards), and the
stale check runs before the `capture-id-missing` throw.

## P1 — Worker binary compiled for the host arch (`scripts/build-worker.mjs`)

`worker-build-target.txt` — `bun build --compile` without `--target` emits
the HOST binary: on Linux the packaged `worker/dictation-worker.exe` is an
ELF (magic `7f 45`), not a PE — yet `package.manifest.json` declares
worker → windows/x86_64 only, and `check-kspkg` cannot tell the difference
(it hashes whatever bytes were built). Fix: `--target bun-windows-x64`;
verified offline, produces `MZ` header.

---

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
