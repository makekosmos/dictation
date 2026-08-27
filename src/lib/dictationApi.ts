export type DictationConfig = {
  hotkey: string;
  language: string;
  injectMode: "auto_paste" | "clipboard_only";
  provider: "groq" | "local";
  model: string;
  localModelId: string;
  providerEnabled: boolean;
};

export type DictationOperation =
  | "dictation.get_state"
  | "dictation.get_config"
  | "dictation.list_local_models"
  | "dictation.update_config"
  | "dictation.start_recording"
  | "dictation.cancel";

export type DictationBridge = {
  request(operation: DictationOperation, params?: Record<string, unknown>): Promise<unknown>;
};

export function createLegacyDictationBridge(bridge: DictationBridge): DictationBridge {
  return {
    async request(operation, params) {
      const result = await bridge.request(operation, params);
      const response = asObject(result);
      if (typeof response.ok === "boolean") return response;
      return { ok: true, data: result };
    },
  };
}

export type DictationState = {
  state?: string;
  microphonePermission?: "unknown" | "granted" | "denied" | "prompt";
};

export type LocalModel = {
  id: string;
  name: string;
  transcriptionSupported: boolean;
  directory: boolean;
  downloaded: boolean;
};

export type LocalModels = {
  commandInstalled: boolean;
  models: LocalModel[];
};

export type DictationApi = {
  getConfig(): Promise<{ config: Partial<DictationConfig>; hasApiKey: boolean }>;
  listLocalModels(): Promise<LocalModels>;
  getState(): Promise<DictationState>;
  updateConfig(config: DictationConfig): Promise<void>;
  startRecording(): Promise<void>;
  cancel(): Promise<void>;
};

export class DictationApiError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "DictationApiError";
  }
}

function asObject(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" ? (value as Record<string, unknown>) : {};
}

function sanitizeConfig(value: unknown): Partial<DictationConfig> {
  const input = asObject(value);
  const config: Partial<DictationConfig> = {};
  if (typeof input.hotkey === "string") config.hotkey = input.hotkey;
  if (typeof input.language === "string") config.language = input.language;
  if (input.injectMode === "auto_paste" || input.injectMode === "clipboard_only")
    config.injectMode = input.injectMode;
  if (input.provider === "groq" || input.provider === "local") config.provider = input.provider;
  if (typeof input.model === "string") config.model = input.model;
  if (typeof input.localModelId === "string") config.localModelId = input.localModelId;
  else if (typeof input.localModel === "string") config.localModelId = input.localModel;
  if (typeof input.providerEnabled === "boolean") config.providerEnabled = input.providerEnabled;
  return config;
}

function sanitizeState(value: unknown): DictationState {
  const input = asObject(value);
  const state: DictationState = {};
  if (typeof input.state === "string") state.state = input.state;
  if (
    input.microphonePermission === "unknown" ||
    input.microphonePermission === "granted" ||
    input.microphonePermission === "denied" ||
    input.microphonePermission === "prompt"
  ) {
    state.microphonePermission = input.microphonePermission;
  }
  return state;
}

function sanitizeLocalModels(value: unknown): LocalModels {
  const input = asObject(value);
  const models = Array.isArray(input.models)
    ? input.models.flatMap((item) => {
        const model = asObject(item);
        if (typeof model.id !== "string" || typeof model.name !== "string") return [];
        return [
          {
            id: model.id,
            name: model.name,
            transcriptionSupported: model.transcriptionSupported === true,
            directory: model.directory === true,
            downloaded: model.downloaded === true,
          },
        ];
      })
    : [];
  return { commandInstalled: input.commandInstalled === true, models };
}

async function request<T>(
  bridge: DictationBridge,
  operation: DictationOperation,
  params: Record<string, unknown> = {},
) {
  let result: unknown;
  try {
    result = await bridge.request(operation, params);
  } catch {
    throw new DictationApiError("Сервис диктовки недоступен");
  }
  const response = asObject(result);
  if (response.ok === false) {
    throw new DictationApiError(
      typeof response.error === "string"
        ? response.error
        : typeof response.message === "string"
          ? response.message
          : "Операция диктовки отклонена",
    );
  }
  if (response.ok !== true) throw new DictationApiError("Некорректный ответ сервиса диктовки");
  return response.data as T;
}

export function createDictationApi(bridge: DictationBridge | undefined): DictationApi | null {
  if (!bridge) return null;
  return {
    async getConfig() {
      const data = asObject(await request(bridge, "dictation.get_config"));
      return { config: sanitizeConfig(data.config), hasApiKey: data.hasApiKey === true };
    },
    async getState() {
      return sanitizeState(await request(bridge, "dictation.get_state"));
    },
    async listLocalModels() {
      return sanitizeLocalModels(await request(bridge, "dictation.list_local_models"));
    },
    async updateConfig(config) {
      await request(bridge, "dictation.update_config", sanitizeConfig(config));
    },
    async startRecording() {
      await request(bridge, "dictation.start_recording");
    },
    async cancel() {
      await request(bridge, "dictation.cancel");
    },
  };
}
