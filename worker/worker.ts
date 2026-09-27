export const DICTATION_OPERATIONS = [
  "dictation.capture.start",
  "dictation.capture.stop",
  "dictation.speech.transcribe",
  "dictation.input.insert_text",
  "dictation.window.foreground",
  "dictation.lifecycle.set_autostart",
] as const;

type Operation = (typeof DICTATION_OPERATIONS)[number];
type Message = Record<string, unknown>;

export class DictationWorker {
  private state = "idle";
  private captureId: string | null = null;
  private windowId: string | null = null;
  private stopAfterStart = false;
  private startSeq = 0;
  private token = "";
  private generation = 0;
  private packageId = "";
  private version = "";
  private hash = "";
  private pid = 0;
  private pending = new Map<string, (message: Message) => void>();
  private nextId = 1;
  private heartbeat: ReturnType<typeof setInterval> | null = null;

  constructor(private readonly emit: (message: Message) => void) {}

  bootstrap(): void {
    this.emit({
      method: "worker.hello",
      package_id: this.packageId,
      version: this.version,
      hash: this.hash,
      pid: this.pid,
      api_version: 1,
      token: this.token,
    });
  }

  cancel(): void {
    // Invalidate any in-flight startCapture: a stale continuation must not
    // adopt a session owned by a newer trigger.
    this.startSeq += 1;
    this.stopAfterStart = false;
    const captureId = this.captureId;
    this.captureId = null;
    this.windowId = null;
    if (captureId) {
      // Tell Engine to drop the live session — forgetting the id alone leaves
      // the capture slot busy forever (the stdin pump also reaches cancel()
      // on malformed lines, mid-capture).
      void this.call("dictation.capture.stop", { captureId }).catch(() => {});
    }
    if (this.state !== "idle") this.setState("idle");
  }

  async message(message: Message): Promise<void> {
    if (message.method === "worker.bootstrap") {
      this.token = typeof message.token === "string" ? message.token : "";
      this.generation = typeof message.generation === "number" ? message.generation : 0;
      this.packageId = typeof message.package_id === "string" ? message.package_id : "";
      this.version = typeof message.version === "string" ? message.version : "";
      this.hash = typeof message.hash === "string" ? message.hash : "";
      this.pid = typeof message.pid === "number" ? message.pid : 0;
      this.bootstrap();
      this.startHeartbeats();
      return;
    }
    if (message.method === "worker.result" && typeof message.id === "string") {
      this.pending.get(message.id)?.(message);
      this.pending.delete(message.id);
      return;
    }
    if (message.method !== "worker.invoke") return;
    const invokeId = typeof message.id === "string" ? message.id : null;
    const reply = (ok: boolean, result?: Message, error?: string) => {
      if (!invokeId) return;
      this.emit({
        method: "worker.result",
        id: invokeId,
        ok,
        result: result ?? null,
        error: error ?? null,
      });
    };
    if (message.operation === "dictation.cancel") {
      this.cancel();
      reply(true, { state: this.state });
      return;
    }
    if (message.operation !== "dictation.trigger") {
      this.emit({ method: "worker.error", error: "unknown-operation" });
      reply(false, undefined, "unknown-operation");
      return;
    }
    const params = isObject(message.params) ? message.params : {};
    const kind = params.kind === "ptt" ? "ptt" : "toggle";
    const phase = params.phase === "up" ? "up" : "down";
    try {
      if (kind === "ptt" && phase === "up") {
        if (this.state === "capturing") await this.finishCapture();
        else if (this.state === "starting") this.stopAfterStart = true;
      } else if (phase === "down" && this.state === "idle") {
        await this.startCapture();
      } else if (kind === "toggle" && phase === "down" && this.state === "capturing") {
        await this.finishCapture();
      }
      reply(true, { state: this.state });
    } catch (error) {
      this.cancel();
      const code = error instanceof Error ? error.message : "operation-failed";
      this.emit({
        method: "worker.event",
        event: "dictation.error",
        data: {
          code,
          retryable: true,
        },
      });
      reply(false, undefined, code);
    }
  }

  private async startCapture(): Promise<void> {
    // Synchronous transition out of "idle": a trigger that lands while the
    // RPCs below are in flight must not start a parallel Engine session (the
    // second window.foreground would steal the one-shot inject token and the
    // second capture.start is rejected "busy").
    this.setState("starting");
    const seq = ++this.startSeq;
    const foreground = await this.call("dictation.window.foreground", {});
    if (seq !== this.startSeq) return; // cancelled or superseded mid-flight
    const windowId =
      isObject(foreground.result) && typeof foreground.result.windowId === "string"
        ? foreground.result.windowId
        : null;
    if (!windowId) throw new Error("window-id-missing");
    const started = await this.call("dictation.capture.start", {});
    const captureId =
      isObject(started.result) && typeof started.result.captureId === "string"
        ? started.result.captureId
        : null;
    if (!captureId) throw new Error("capture-id-missing");
    if (seq !== this.startSeq || this.state !== "starting") {
      // cancel() or a newer trigger ran while capture.start was in flight —
      // stop the orphaned Engine session instead of leaving the capture slot
      // busy forever.
      await this.call("dictation.capture.stop", { captureId });
      return;
    }
    this.windowId = windowId;
    this.captureId = captureId;
    this.setState("capturing");
    if (this.stopAfterStart) {
      // PTT released before capture.start landed — finish immediately.
      this.stopAfterStart = false;
      await this.finishCapture();
    }
  }

  private async finishCapture(): Promise<void> {
    const captureId = this.captureId;
    if (!captureId) return;
    this.setState("transcribing");
    const stopped = await this.call("dictation.capture.stop", { captureId });
    // A stale continuation must not touch shared state: cancel() cleared the
    // fields, or a newer session already installed its own captureId/windowId
    // — writing through would insert old text into the new session's window
    // and wipe its live capture.
    if (this.captureId !== captureId) return;
    const audio = isObject(stopped.result) ? stopped.result : {};
    const audioB64 = typeof audio.audioB64 === "string" ? audio.audioB64 : null;
    if (audioB64 === null) throw new Error("audio-missing");
    if (!audioB64) {
      // Zero-length recording (e.g. PTT released before audio buffered) —
      // nothing to transcribe; finish silently instead of raising an error.
      this.captureId = null;
      this.windowId = null;
      this.setState("idle");
      return;
    }
    const transcription = await this.call("dictation.speech.transcribe", {
      audioB64,
      durationSec: typeof audio.durationMs === "number" ? Math.max(0, audio.durationMs) / 1000 : 0,
      delivery: "text_only",
    });
    if (this.captureId !== captureId) return; // cancelled/superseded mid-flight
    const text =
      isObject(transcription.result) && typeof transcription.result.text === "string"
        ? transcription.result.text
        : "";
    if (text) {
      this.setState("inserting");
      const targetWindow = this.windowId;
      if (!targetWindow) throw new Error("window-id-missing");
      await this.call("dictation.input.insert_text", { text, targetWindow });
      if (this.captureId !== captureId) return; // cancelled/superseded mid-flight
    }
    this.captureId = null;
    this.windowId = null;
    this.setState("idle");
  }

  private call(operation: Operation, params: Message): Promise<Message> {
    const id = String(this.nextId++);
    this.emit({
      method: "worker.call",
      id,
      generation: this.generation,
      token: this.token,
      operation: "ark.write",
      params: { operation, params },
    });
    return new Promise((resolve, reject) => {
      this.pending.set(id, (message) => {
        if (message.ok === false) reject(new Error(String(message.error ?? "operation-failed")));
        else resolve(message);
      });
    });
  }

  private setState(state: string): void {
    this.state = state;
  }

  // The supervisor reaps a Running worker whose last_heartbeat is older than
  // 60s — without these beats dictation dies a minute after every start.
  private startHeartbeats(): void {
    if (this.heartbeat !== null) return;
    this.heartbeat = setInterval(() => {
      this.emit({
        method: "worker.heartbeat",
        generation: this.generation,
        token: this.token,
      });
    }, 15_000);
    this.heartbeat.unref?.();
  }
}

function isObject(value: unknown): value is Message {
  return !!value && typeof value === "object";
}

if (import.meta.main) {
  const worker = new DictationWorker((message) =>
    process.stdout.write(`${JSON.stringify(message)}\n`),
  );
  process.stdin.setEncoding("utf8");
  process.stdin.resume();
  let buffer = "";
  process.stdin.on("data", (chunk: string) => {
    buffer += chunk;
    if (buffer.startsWith("\uFEFF")) buffer = buffer.slice(1);
    const lines = buffer.split("\n");
    buffer = lines.pop() ?? "";
    for (const line of lines) {
      if (!line.trim()) continue;
      try {
        void worker.message(JSON.parse(line)).catch(() => {
          worker.cancel();
          process.stdout.write(
            `${JSON.stringify({ method: "worker.event", event: "dictation.error", data: { code: "worker-message-failed", retryable: true } })}\n`,
          );
        });
      } catch {
        worker.cancel();
        process.stdout.write(
          `${JSON.stringify({ method: "worker.event", event: "dictation.error", data: { code: "invalid-json", retryable: false } })}\n`,
        );
      }
    }
  });
}
