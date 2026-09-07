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
  private token = "";
  private generation = 0;
  private pending = new Map<number, (message: Message) => void>();
  private nextId = 1;

  constructor(private readonly emit: (message: Message) => void) {}

  bootstrap(): void {
    this.emit({ method: "worker.hello", api_version: 1 });
    this.emitState();
  }

  cancel(): void {
    this.captureId = null;
    if (this.state !== "idle") this.setState("idle");
  }

  async message(message: Message): Promise<void> {
    if (message.method === "worker.bootstrap") {
      this.token = typeof message.token === "string" ? message.token : "";
      this.generation = typeof message.generation === "number" ? message.generation : 0;
      this.bootstrap();
      return;
    }
    if (message.method === "worker.result" && typeof message.id === "number") {
      this.pending.get(message.id)?.(message);
      this.pending.delete(message.id);
      return;
    }
    if (message.method !== "worker.invoke") return;
    if (message.operation === "dictation.cancel") {
      this.cancel();
      return;
    }
    if (message.operation !== "dictation.trigger") {
      this.emit({ method: "worker.error", error: "unknown-operation" });
      return;
    }
    const params = isObject(message.params) ? message.params : {};
    const kind = params.kind === "ptt" ? "ptt" : "toggle";
    const phase = params.phase === "up" ? "up" : "down";
    if (kind === "ptt" && phase === "up") {
      if (this.state === "capturing") await this.finishCapture();
      return;
    }
    try {
      if (this.state === "idle") await this.startCapture();
      else if (kind === "toggle" && this.state === "capturing") await this.finishCapture();
    } catch (error) {
      this.cancel();
      this.emit({
        method: "worker.event",
        event: "dictation.error",
        data: {
          code: error instanceof Error ? error.message : "operation-failed",
          retryable: true,
        },
      });
    }
  }

  private async startCapture(): Promise<void> {
    await this.call("dictation.window.foreground", {});
    const started = await this.call("dictation.capture.start", {});
    const captureId =
      isObject(started.result) && typeof started.result.capture_id === "string"
        ? started.result.capture_id
        : null;
    if (!captureId) throw new Error("capture-id-missing");
    this.captureId = captureId;
    this.setState("capturing");
  }

  private async finishCapture(): Promise<void> {
    const captureId = this.captureId;
    if (!captureId) return;
    this.setState("transcribing");
    const stopped = await this.call("dictation.capture.stop", { capture_id: captureId });
    const transcription = await this.call("dictation.speech.transcribe", {
      capture_id: captureId,
      result: stopped.result,
    });
    const text =
      isObject(transcription.result) && typeof transcription.result.text === "string"
        ? transcription.result.text
        : "";
    if (text) {
      this.setState("inserting");
      await this.call("dictation.input.insert_text", { text, capture_id: captureId });
    }
    this.captureId = null;
    this.setState("idle");
  }

  private call(operation: Operation, params: Message): Promise<Message> {
    const id = this.nextId++;
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
    this.emitState();
  }

  private emitState(): void {
    this.emit({
      method: "worker.event",
      event: "dictation.state_changed",
      data: { state: this.state },
    });
  }
}

function isObject(value: unknown): value is Message {
  return !!value && typeof value === "object";
}

if (import.meta.main) {
  const worker = new DictationWorker((message) =>
    Bun.write(Bun.stdout, `${JSON.stringify(message)}\n`),
  );
  worker.bootstrap();
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
