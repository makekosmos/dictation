import { describe, expect, it, vi } from "vitest";
import { DICTATION_OPERATIONS, DictationWorker } from "../worker/worker";

function harness(boot = false) {
  const messages: Record<string, unknown>[] = [];
  const worker = new DictationWorker((message) => messages.push(message));
  if (boot) worker.bootstrap();
  return { worker, messages };
}

async function finishCall(
  worker: DictationWorker,
  messages: Record<string, unknown>[],
  result: unknown,
) {
  const call = messages.at(-1)!;
  await worker.message({ method: "worker.result", id: call.id, ok: true, result });
}

const workerCalls = (messages: Record<string, unknown>[]) =>
  messages.filter((m) => m.method === "worker.call");

const callOperations = (messages: Record<string, unknown>[]) =>
  workerCalls(messages).map((m) => (m.params as { operation?: string }).operation);

// Unlike finishCall: invoke replies (`worker.result` outbound) interleave with
// pending calls when two invokes overlap, so resolve the last worker.call.
async function finishLastCall(
  worker: DictationWorker,
  messages: Record<string, unknown>[],
  result: unknown,
) {
  const call = workerCalls(messages).at(-1)!;
  await worker.message({ method: "worker.result", id: call.id, ok: true, result });
}

// Reject the most recent worker.call for one Engine operation.
async function failOperation(
  worker: DictationWorker,
  messages: Record<string, unknown>[],
  operation: string,
  error: string,
) {
  const call = workerCalls(messages)
    .filter((m) => (m.params as { operation?: string }).operation === operation)
    .at(-1)!;
  await worker.message({ method: "worker.result", id: call.id, ok: false, error });
}

// Resolve a specific worker.call by id — needed when two sessions have the
// same Engine operation in flight and the STALE one must be picked.
async function finishCallById(worker: DictationWorker, id: unknown, result: unknown) {
  await worker.message({ method: "worker.result", id, ok: true, result });
}

// Resolve the most recent worker.call for one Engine operation — needed when a
// fire-and-forget (cancel's capture.stop) lands after the call under test.
async function finishOperation(
  worker: DictationWorker,
  messages: Record<string, unknown>[],
  operation: string,
  result: unknown,
) {
  const call = workerCalls(messages)
    .filter((m) => (m.params as { operation?: string }).operation === operation)
    .at(-1)!;
  await worker.message({ method: "worker.result", id: call.id, ok: true, result });
}

describe("dictation.v2 worker", () => {
  it("keeps the exact Engine operation allow-list", () => {
    expect(DICTATION_OPERATIONS).toEqual([
      "dictation.capture.start",
      "dictation.capture.stop",
      "dictation.speech.transcribe",
      "dictation.input.insert_text",
      "dictation.window.foreground",
      "dictation.lifecycle.set_autostart",
    ]);
  });

  it("orchestrates capture, transcription, and insertion", async () => {
    const { worker, messages } = harness();
    await worker.message({
      method: "worker.bootstrap",
      token: "opaque-token",
      generation: 7,
      package_id: "com.kosmos.dictation",
      version: "0.2.5",
      hash: "archive-hash",
      pid: 42,
    });
    expect(messages[0]).toMatchObject({
      method: "worker.hello",
      package_id: "com.kosmos.dictation",
      version: "0.2.5",
      hash: "archive-hash",
      pid: 42,
      api_version: 1,
      token: "opaque-token",
    });
    const run = worker.message({
      method: "worker.invoke",
      operation: "dictation.trigger",
      params: {},
    });
    await finishCall(worker, messages, { windowId: "window-1" });
    expect(messages.at(-1)).toMatchObject({ token: "opaque-token", generation: 7 });
    await finishCall(worker, messages, { captureId: "opaque-capture" });
    await run;
    const finish = worker.message({
      method: "worker.invoke",
      operation: "dictation.trigger",
      params: {},
    });
    await finishCall(worker, messages, {
      captureId: "opaque-capture",
      audioB64: "UklGRg==",
      durationMs: 1250,
    });
    expect(messages.at(-1)).toMatchObject({
      params: {
        operation: "dictation.speech.transcribe",
        params: { audioB64: "UklGRg==", durationSec: 1.25, delivery: "text_only" },
      },
    });
    await finishCall(worker, messages, { text: "Привет" });
    expect(messages.at(-1)).toMatchObject({
      params: {
        operation: "dictation.input.insert_text",
        params: { text: "Привет", targetWindow: "window-1" },
      },
    });
    await finishCall(worker, messages, {});
    await finish;
    expect(messages.filter((item) => item.method === "worker.event")).toHaveLength(0);
  });

  it("rejects unknown worker invokes", async () => {
    const { worker, messages } = harness();
    await worker.message({ method: "worker.invoke", operation: "dictation.unknown" });
    expect(messages.at(-1)).toEqual({ method: "worker.error", error: "unknown-operation" });
  });

  it("makes cancellation idempotent", () => {
    const { worker, messages } = harness();
    worker.cancel();
    worker.cancel();
    expect(messages).toEqual([]);
  });

  it("heartbeats so the supervisor does not reap a running worker", async () => {
    vi.useFakeTimers();
    try {
      const { worker, messages } = harness();
      await worker.message({
        method: "worker.bootstrap",
        token: "opaque-token",
        generation: 7,
      });
      messages.length = 0;
      await vi.advanceTimersByTimeAsync(65_000);
      const beats = messages.filter((m) => m.method === "worker.heartbeat");
      expect(beats.length).toBeGreaterThan(0);
      for (const beat of beats) {
        expect(beat).toMatchObject({ generation: 7, token: "opaque-token" });
      }
    } finally {
      vi.useRealTimers();
    }
  });

  it("ignores a second trigger while a capture is still starting", async () => {
    const { worker, messages } = harness();
    await worker.message({ method: "worker.bootstrap", token: "t", generation: 1 });
    const first = worker.message({
      method: "worker.invoke",
      id: "invoke-1",
      operation: "dictation.trigger",
      params: {},
    });
    const second = worker.message({
      method: "worker.invoke",
      id: "invoke-2",
      operation: "dictation.trigger",
      params: {},
    });
    // The second trigger must not fan out into a second Engine session:
    // a duplicate window.foreground would steal the one-shot foreground
    // token and the duplicate capture.start would be rejected "busy".
    expect(workerCalls(messages)).toHaveLength(1);
    await finishLastCall(worker, messages, { windowId: "window-1" });
    await finishLastCall(worker, messages, { captureId: "capture-1" });
    await Promise.all([first, second]);
    expect(callOperations(messages)).toEqual([
      "dictation.window.foreground",
      "dictation.capture.start",
    ]);
    expect(messages.find((m) => m.method === "worker.result" && m.id === "invoke-2")).toMatchObject(
      { ok: true },
    );
  });

  it("finishes the capture when push-to-talk is released mid-start", async () => {
    const { worker, messages } = harness();
    await worker.message({ method: "worker.bootstrap", token: "t", generation: 1 });
    const down = worker.message({
      method: "worker.invoke",
      id: "invoke-down",
      operation: "dictation.trigger",
      params: { kind: "ptt", phase: "down" },
    });
    const up = worker.message({
      method: "worker.invoke",
      id: "invoke-up",
      operation: "dictation.trigger",
      params: { kind: "ptt", phase: "up" },
    });
    await finishLastCall(worker, messages, { windowId: "window-1" });
    await finishLastCall(worker, messages, { captureId: "capture-1" });
    await finishLastCall(worker, messages, {
      captureId: "capture-1",
      audioB64: "UklGRg==",
      durationMs: 60,
    });
    await finishLastCall(worker, messages, { text: "" });
    await Promise.all([down, up]);
    expect(callOperations(messages)).toEqual([
      "dictation.window.foreground",
      "dictation.capture.start",
      "dictation.capture.stop",
      "dictation.speech.transcribe",
    ]);
    expect(
      messages.find((m) => m.method === "worker.result" && m.id === "invoke-down"),
    ).toMatchObject({ ok: true, result: { state: "idle" } });
  });

  it("stops a live Engine capture when cancelled mid-capture", async () => {
    const { worker, messages } = harness();
    await worker.message({ method: "worker.bootstrap", token: "t", generation: 1 });
    const run = worker.message({
      method: "worker.invoke",
      id: "invoke-1",
      operation: "dictation.trigger",
      params: {},
    });
    await finishLastCall(worker, messages, { windowId: "window-1" });
    await finishLastCall(worker, messages, { captureId: "capture-1" });
    await run;
    messages.length = 0;
    await worker.message({
      method: "worker.invoke",
      id: "invoke-cancel",
      operation: "dictation.cancel",
      params: {},
    });
    // Forgetting the id alone would leave the Engine capture slot busy
    // forever — every later capture.start would fail.
    expect(callOperations(messages)).toEqual(["dictation.capture.stop"]);
    expect(workerCalls(messages).at(-1)?.params).toMatchObject({
      params: { captureId: "capture-1" },
    });
  });

  it("stops the Engine capture that resolves after a mid-start cancel", async () => {
    const { worker, messages } = harness();
    await worker.message({ method: "worker.bootstrap", token: "t", generation: 1 });
    const run = worker.message({
      method: "worker.invoke",
      id: "invoke-1",
      operation: "dictation.trigger",
      params: {},
    });
    await finishLastCall(worker, messages, { windowId: "window-1" });
    await worker.message({
      method: "worker.invoke",
      id: "invoke-cancel",
      operation: "dictation.cancel",
      params: {},
    });
    // capture.start resolves after cancel(): the worker must stop the
    // orphaned Engine session rather than adopt it.
    await finishLastCall(worker, messages, { captureId: "capture-late" });
    // The orphan cleanup issues its own capture.stop call — resolve it.
    await finishLastCall(worker, messages, { captureId: "capture-late" });
    await run;
    expect(callOperations(messages)).toEqual([
      "dictation.window.foreground",
      "dictation.capture.start",
      "dictation.capture.stop",
    ]);
    expect(workerCalls(messages).at(-1)?.params).toMatchObject({
      params: { captureId: "capture-late" },
    });
  });

  it("ignores an up-phase toggle trigger instead of double-toggling", async () => {
    const { worker, messages } = harness();
    await worker.message({ method: "worker.bootstrap", token: "t", generation: 1 });
    await worker.message({
      method: "worker.invoke",
      id: "invoke-up",
      operation: "dictation.trigger",
      params: { kind: "toggle", phase: "up" },
    });
    expect(callOperations(messages)).toEqual([]);
  });

  it("finishes silently when cancelled while transcribe is in flight", async () => {
    const { worker, messages } = harness();
    await worker.message({ method: "worker.bootstrap", token: "t", generation: 1 });
    const run = worker.message({
      method: "worker.invoke",
      id: "invoke-start",
      operation: "dictation.trigger",
      params: {},
    });
    await finishLastCall(worker, messages, { windowId: "window-1" });
    await finishLastCall(worker, messages, { captureId: "capture-1" });
    await run;
    const finish = worker.message({
      method: "worker.invoke",
      id: "invoke-finish",
      operation: "dictation.trigger",
      params: {},
    });
    await finishLastCall(worker, messages, {
      captureId: "capture-1",
      audioB64: "UklGRg==",
      durationMs: 900,
    });
    // speech.transcribe is in flight when the cancel lands.
    await worker.message({
      method: "worker.invoke",
      id: "invoke-cancel",
      operation: "dictation.cancel",
      params: {},
    });
    await finishOperation(worker, messages, "dictation.speech.transcribe", { text: "текст" });
    await finish;
    // A cancelled session must not surface an error or reach insert_text:
    // cancel() already cleared the fields its stale continuation would use.
    expect(messages.filter((m) => m.event === "dictation.error")).toHaveLength(0);
    expect(callOperations(messages)).not.toContain("dictation.input.insert_text");
    expect(
      messages.find((m) => m.method === "worker.result" && m.id === "invoke-finish"),
    ).toMatchObject({ ok: true });
  });

  it("does not leak a superseded session's text into the next session", async () => {
    const { worker, messages } = harness();
    await worker.message({ method: "worker.bootstrap", token: "t", generation: 1 });
    const first = worker.message({
      method: "worker.invoke",
      id: "invoke-1",
      operation: "dictation.trigger",
      params: {},
    });
    await finishLastCall(worker, messages, { windowId: "window-1" });
    await finishLastCall(worker, messages, { captureId: "capture-1" });
    await first;
    const finish = worker.message({
      method: "worker.invoke",
      id: "invoke-2",
      operation: "dictation.trigger",
      params: {},
    });
    await finishLastCall(worker, messages, {
      captureId: "capture-1",
      audioB64: "UklGRg==",
      durationMs: 900,
    });
    // Transcribe in flight → cancel → a new session starts before the stale
    // reply lands.
    await worker.message({
      method: "worker.invoke",
      id: "invoke-cancel",
      operation: "dictation.cancel",
      params: {},
    });
    const second = worker.message({
      method: "worker.invoke",
      id: "invoke-3",
      operation: "dictation.trigger",
      params: {},
    });
    await finishOperation(worker, messages, "dictation.window.foreground", {
      windowId: "window-2",
    });
    await finishOperation(worker, messages, "dictation.capture.start", { captureId: "capture-2" });
    await finishOperation(worker, messages, "dictation.speech.transcribe", {
      text: "устаревший текст",
    });
    await Promise.all([finish, second]);
    // The stale continuation used to read the NEW session's windowId — the
    // cancelled transcript would be pasted into the new target window and the
    // live capture state wiped.
    expect(callOperations(messages)).not.toContain("dictation.input.insert_text");
    expect(messages.filter((m) => m.event === "dictation.error")).toHaveLength(0);
    // Session 2 still owns the capture: its stop finishes it.
    const stop2 = worker.message({
      method: "worker.invoke",
      id: "invoke-4",
      operation: "dictation.trigger",
      params: {},
    });
    await finishLastCall(worker, messages, { captureId: "capture-2", audioB64: "", durationMs: 0 });
    await stop2;
    expect(callOperations(messages).at(-1)).toBe("dictation.capture.stop");
  });

  it("resets state and emits a safe error when Engine rejects a call", async () => {
    const { worker, messages } = harness();
    const run = worker.message({ method: "worker.bootstrap", token: "t", generation: 1 });
    await run;
    const invoke = worker.message({
      method: "worker.invoke",
      operation: "dictation.trigger",
      params: {},
    });
    const call = messages.at(-1)!;
    await worker.message({ method: "worker.result", id: call.id, ok: false, error: "denied" });
    await invoke;
    expect(messages.at(-1)).toMatchObject({ event: "dictation.error", data: { code: "denied" } });
  });

  it("does not let a superseded session's transcribe failure cancel the live session", async () => {
    const { worker, messages } = harness();
    await worker.message({ method: "worker.bootstrap", token: "t", generation: 1 });
    const first = worker.message({
      method: "worker.invoke",
      id: "invoke-1",
      operation: "dictation.trigger",
      params: {},
    });
    await finishLastCall(worker, messages, { windowId: "window-1" });
    await finishLastCall(worker, messages, { captureId: "capture-1" });
    await first;
    const finish = worker.message({
      method: "worker.invoke",
      id: "invoke-2",
      operation: "dictation.trigger",
      params: {},
    });
    await finishLastCall(worker, messages, {
      captureId: "capture-1",
      audioB64: "UklGRg==",
      durationMs: 900,
    });
    // Transcribe in flight → cancel → a new session starts before the stale
    // reply lands.
    await worker.message({
      method: "worker.invoke",
      id: "invoke-cancel",
      operation: "dictation.cancel",
      params: {},
    });
    const second = worker.message({
      method: "worker.invoke",
      id: "invoke-3",
      operation: "dictation.trigger",
      params: {},
    });
    await finishOperation(worker, messages, "dictation.window.foreground", {
      windowId: "window-2",
    });
    await finishOperation(worker, messages, "dictation.capture.start", { captureId: "capture-2" });
    // Session 2 is now capturing. The stale transcribe REJECTS — its failure
    // belongs to the dead session and must not touch the live one.
    await failOperation(worker, messages, "dictation.speech.transcribe", "engine-busy");
    await Promise.all([finish, second]);
    expect(
      workerCalls(messages).filter(
        (m) =>
          (m.params as { operation?: string }).operation === "dictation.capture.stop" &&
          (m.params as { params?: { captureId?: string } }).params?.captureId === "capture-2",
      ),
    ).toHaveLength(0);
    expect(messages.filter((m) => m.event === "dictation.error")).toHaveLength(0);
    // Session 2 still owns its capture: a fresh trigger finishes it.
    const stop2 = worker.message({
      method: "worker.invoke",
      id: "invoke-4",
      operation: "dictation.trigger",
      params: {},
    });
    await finishLastCall(worker, messages, { captureId: "capture-2", audioB64: "", durationMs: 0 });
    await stop2;
    expect(callOperations(messages).at(-1)).toBe("dictation.capture.stop");
    expect(messages.find((m) => m.method === "worker.result" && m.id === "invoke-4")).toMatchObject(
      { ok: true, result: { state: "idle" } },
    );
  });

  it("does not let a superseded start's failure cancel the live session", async () => {
    const { worker, messages } = harness();
    await worker.message({ method: "worker.bootstrap", token: "t", generation: 1 });
    const first = worker.message({
      method: "worker.invoke",
      id: "invoke-1",
      operation: "dictation.trigger",
      params: {},
    });
    await finishLastCall(worker, messages, { windowId: "window-1" });
    // Session 1's capture.start is in flight when the cancel + retrigger land.
    await worker.message({
      method: "worker.invoke",
      id: "invoke-cancel",
      operation: "dictation.cancel",
      params: {},
    });
    const second = worker.message({
      method: "worker.invoke",
      id: "invoke-2",
      operation: "dictation.trigger",
      params: {},
    });
    const foregrounds = workerCalls(messages).filter(
      (m) => (m.params as { operation?: string }).operation === "dictation.window.foreground",
    );
    await finishCallById(worker, foregrounds.at(-1)!.id, { windowId: "window-2" });
    // Both capture.start calls are in flight: stale session 1's rejects.
    const starts = workerCalls(messages).filter(
      (m) => (m.params as { operation?: string }).operation === "dictation.capture.start",
    );
    expect(starts).toHaveLength(2);
    await worker.message({
      method: "worker.result",
      id: starts[0].id,
      ok: false,
      error: "engine-busy",
    });
    await finishCallById(worker, starts[1].id, { captureId: "capture-2" });
    // Let any stray capture.stop settle so the invokes resolve either way.
    for (const stop of workerCalls(messages).filter(
      (m) => (m.params as { operation?: string }).operation === "dictation.capture.stop",
    )) {
      await finishCallById(worker, stop.id, {});
    }
    await Promise.all([first, second]);
    // The stale rejection must not cancel() — session 2's capture stays live
    // (no orphan stop for its id) and no bogus dictation.error surfaces.
    expect(messages.filter((m) => m.event === "dictation.error")).toHaveLength(0);
    expect(
      workerCalls(messages).filter(
        (m) =>
          (m.params as { operation?: string }).operation === "dictation.capture.stop" &&
          (m.params as { params?: { captureId?: string } }).params?.captureId === "capture-2",
      ),
    ).toHaveLength(0);
    expect(messages.find((m) => m.method === "worker.result" && m.id === "invoke-2")).toMatchObject(
      { ok: true, result: { state: "capturing" } },
    );
  });

  it("lets a superseded start's id-less reply pass without throwing capture-id-missing", async () => {
    const { worker, messages } = harness();
    await worker.message({ method: "worker.bootstrap", token: "t", generation: 1 });
    const first = worker.message({
      method: "worker.invoke",
      id: "invoke-1",
      operation: "dictation.trigger",
      params: {},
    });
    await finishLastCall(worker, messages, { windowId: "window-1" });
    await worker.message({
      method: "worker.invoke",
      id: "invoke-cancel",
      operation: "dictation.cancel",
      params: {},
    });
    const second = worker.message({
      method: "worker.invoke",
      id: "invoke-2",
      operation: "dictation.trigger",
      params: {},
    });
    const foregrounds = workerCalls(messages).filter(
      (m) => (m.params as { operation?: string }).operation === "dictation.window.foreground",
    );
    await finishCallById(worker, foregrounds.at(-1)!.id, { windowId: "window-2" });
    const starts = workerCalls(messages).filter(
      (m) => (m.params as { operation?: string }).operation === "dictation.capture.start",
    );
    expect(starts).toHaveLength(2);
    // Stale session 1's capture.start lands malformed (no captureId). If the
    // stale check ran AFTER the capture-id-missing throw, this would reject
    // into message()'s catch and cancel() session 2.
    await finishCallById(worker, starts[0].id, {});
    await finishCallById(worker, starts[1].id, { captureId: "capture-2" });
    await Promise.all([first, second]);
    expect(messages.filter((m) => m.event === "dictation.error")).toHaveLength(0);
    expect(
      workerCalls(messages).filter(
        (m) =>
          (m.params as { operation?: string }).operation === "dictation.capture.stop" &&
          (m.params as { params?: { captureId?: string } }).params?.captureId === "capture-2",
      ),
    ).toHaveLength(0);
    expect(messages.find((m) => m.method === "worker.result" && m.id === "invoke-2")).toMatchObject(
      { ok: true, result: { state: "capturing" } },
    );
  });
});
