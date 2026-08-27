import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { createDictationApi, createLegacyDictationBridge } from "../src/lib/dictationApi";

const readJson = (file: string) => JSON.parse(readFileSync(file, "utf8"));

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
  it("keeps package identity, permissions, and legacy migration aligned", () => {
    const packageJson = readJson("package.json");
    const packageManifest = readJson("package.manifest.json");
    const legacyManifest = readJson("manifest.json");
    const compatibility = readJson("compatibility.json");

    expect(packageJson.version).toBe(packageManifest.version);
    expect(packageJson.version).toBe(legacyManifest.version);
    expect(packageManifest.id).toBe("com.kosmos.dictation");
    expect(legacyManifest.id).toBe("dictation");
    expect(compatibility.current).toEqual({
      app_id: packageManifest.id,
      extension_id: legacyManifest.id,
      operation_namespace: "dictation",
    });
    expect(compatibility.legacy.permissions).toEqual(["dictation.control"]);
    expect(compatibility.legacy.data.preserve).toEqual([
      "config",
      "credentials",
      "models",
      "tools",
      "permissions",
    ]);
    expect(compatibility.legacy.remove_after).toBe("1.0.0");
  });

  it("exposes only the manifest-scoped lifecycle and config methods", async () => {
    const calls: string[] = [];
    const params: Record<string, unknown>[] = [];
    const api = createDictationApi({
      request: async (operation, requestParams) => {
        calls.push(operation);
        params.push(requestParams ?? {});
        if (operation === "dictation.get_state") {
          return {
            ok: true,
            data: { state: "recording", microphonePermission: "granted", apiKey: "secret" },
          };
        }
        if (operation === "dictation.list_local_models") {
          return { ok: true, data: { commandInstalled: true, models: [] } };
        }
        return { ok: true, data: { config, hasApiKey: true, apiKey: "secret" } };
      },
    })!;

    expect(Object.keys(api).sort()).toEqual([
      "cancel",
      "getConfig",
      "getState",
      "listLocalModels",
      "startRecording",
      "updateConfig",
    ]);
    await api.getConfig();
    await api.getState();
    await api.listLocalModels();
    await api.updateConfig({ ...config, apiKey: "secret" } as typeof config);
    await api.startRecording();
    await api.cancel();
    expect(calls).toEqual([
      "dictation.get_config",
      "dictation.get_state",
      "dictation.list_local_models",
      "dictation.update_config",
      "dictation.start_recording",
      "dictation.cancel",
    ]);
    expect(params[3]).toEqual(config);
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

  it("adapts legacy raw responses without widening the renderer API", async () => {
    const legacy = createLegacyDictationBridge({
      request: async (operation) =>
        operation === "dictation.get_config"
          ? { config, apiKey: "secret", hasApiKey: true }
          : { ok: false, error: "Отклонено" },
    });
    const api = createDictationApi(legacy)!;

    await expect(api.getConfig()).resolves.toEqual({ config, hasApiKey: true });
    await expect(api.startRecording()).rejects.toThrow("Отклонено");
  });
});
