import { describe, expect, it } from "vitest";
import { createDictationApi } from "../src/lib/dictationApi";

const config = {
  hotkey: "Ctrl+Shift+;",
  language: "ru",
  injectMode: "auto_paste" as const,
  provider: "groq" as const,
  model: "whisper-large-v3-turbo",
  localModelId: "small",
  providerEnabled: true,
};

describe("Dictation Cortex contract", () => {
  it("exposes only the manifest-scoped lifecycle and config methods", async () => {
    const calls: string[] = [];
    const params: Record<string, unknown>[] = [];
    const api = createDictationApi({
      request: async (operation, requestParams) => {
        calls.push(operation);
        params.push(requestParams ?? {});
        return operation === "dictation.get_state"
          ? {
              ok: true,
              data: { state: "recording", microphonePermission: "granted", apiKey: "secret" },
            }
          : { ok: true, data: { config, hasApiKey: true, apiKey: "secret" } };
      },
    })!;

    expect(Object.keys(api).sort()).toEqual([
      "cancel",
      "getConfig",
      "getState",
      "startRecording",
      "updateConfig",
    ]);
    await api.getConfig();
    await api.getState();
    await api.updateConfig({ ...config, apiKey: "secret" } as typeof config);
    await api.startRecording();
    await api.cancel();
    expect(calls).toEqual([
      "dictation.get_config",
      "dictation.get_state",
      "dictation.update_config",
      "dictation.start_recording",
      "dictation.cancel",
    ]);
    expect(params[2]).toEqual(config);
  });

  it("returns permission status and config without credential or native fields", async () => {
    const api = createDictationApi({
      request: async (operation) =>
        operation === "dictation.get_state"
          ? {
              ok: true,
              data: { state: "idle", microphonePermission: "denied", apiKey: "secret", hwnd: 1 },
            }
          : {
              ok: true,
              data: { config, hasApiKey: true, apiKey: "secret", credentialHandle: "secret" },
            },
    })!;

    await expect(api.getConfig()).resolves.toEqual({ config, hasApiKey: true });
    await expect(api.getState()).resolves.toEqual({
      state: "idle",
      microphonePermission: "denied",
    });
  });

  it("turns denied native and failed transcription/injection responses into safe errors", async () => {
    const api = createDictationApi({
      request: async (operation) => {
        if (operation === "dictation.start_recording")
          return { ok: false, error: "Микрофон недоступен" };
        throw new Error("native secret");
      },
    })!;

    await expect(api.startRecording()).rejects.toThrow("Микрофон недоступен");
    await expect(api.cancel()).rejects.toThrow("Сервис диктовки недоступен");
  });
});
