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
  private token = "";
  private generation = 0;
  private packageId = "";
  private version = "";
  private hash = "";
  private pid = 0;
  private pending = new Map<string, (message: Message) => void>();
  private nextId = 1;

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
    this.captureId = null;
    this.windowId = null;
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
    if (kind === "ptt" && phase === "up") {
      if (this.state === "capturing") await this.finishCapture();
      reply(true, { state: this.state });
      return;
    }
    try {
      if (this.state === "idle") await this.startCapture();
      else if (kind === "toggle" && this.state === "capturing") await this.finishCapture();
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
    const foreground = await this.call("dictation.window.foreground", {});
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
    this.windowId = windowId;
    this.captureId = captureId;
    this.setState("capturing");
  }

  private async finishCapture(): Promise<void> {
    const captureId = this.captureId;
    if (!captureId) return;
    this.setState("transcribing");
    const stopped = await this.call("dictation.capture.stop", { captureId });
    const audio = isObject(stopped.result) ? stopped.result : {};
    const audioB64 = typeof audio.audioB64 === "string" ? audio.audioB64 : null;
    if (!audioB64) throw new Error("audio-missing");
    const transcription = await this.call("dictation.speech.transcribe", {
      audioB64,
      durationSec: typeof audio.durationMs === "number" ? Math.max(0, audio.durationMs) / 1000 : 0,
      delivery: "text_only",
    });
    const text =
      isObject(transcription.result) && typeof transcription.result.text === "string"
        ? transcription.result.text
        : "";
    if (text) {
      this.setState("inserting");
      const targetWindow = this.windowId;
      if (!targetWindow) throw new Error("window-id-missing");
      await this.call("dictation.input.insert_text", { text, targetWindow });
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
}

function isObject(value: unknown): value is Message {
  return !!value && typeof value === "object";
}

if (import.meta.main) {
  const worker = new DictationWorker((message) =>
    Bun.write(Bun.stdout, `${JSON.stringify(message)}\n`),
  );
  const decoder = new TextDecoder();
  let buffer = "";
  for await (const chunk of Bun.stdin.stream()) {
    buffer += decoder.decode(chunk, { stream: true });
    const lines = buffer.split("\n");
    buffer = lines.pop() ?? "";
    for (const line of lines) {
      if (!line.trim()) continue;
      try {
        void worker.message(JSON.parse(line)).catch(() => {
          worker.cancel();
          Bun.write(
            Bun.stdout,
            `${JSON.stringify({ method: "worker.event", event: "dictation.error", data: { code: "worker-message-failed", retryable: true } })}\n`,
          );
        });
      } catch {
        worker.cancel();
        Bun.write(
          Bun.stdout,
          `${JSON.stringify({ method: "worker.event", event: "dictation.error", data: { code: "invalid-json", retryable: false } })}\n`,
        );
      }
    }
  }
}
