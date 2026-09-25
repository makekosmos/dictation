import { describe, expect, it } from "vitest";
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
});
