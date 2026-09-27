export {
  DEFAULT_DICTATION_CFG,
  DEFAULT_DICTATION_STATS,
  DICTATION_IDLE_UNLOAD_OPTIONS,
  DICTATION_INJECT_OPTIONS,
  DICTATION_LANGUAGE_OPTIONS,
  DICTATION_PROVIDER_OPTIONS,
  DICTATION_TRIGGER_OPTIONS,
  DNS_PROFILE_OPTIONS,
} from "./useDictationConfig.data";
export type {
  ConnectivityReport,
  DictationConfigData,
  DictationLocalModelsSnapshot,
  DictationProvider,
  DictationStatsData,
  DictationVoiceModelOption,
  DnsKind,
} from "./useDictationConfig.data";

import {
  DEFAULT_DICTATION_CFG,
  DEFAULT_DICTATION_STATS,
  type DictationConfigData,
  type DictationLocalModelsSnapshot,
  type DictationStatsData,
  type DictationVoiceModelOption,
  type DictationProvider,
} from "./useDictationConfig.data";

function vkToKeyName(vk: number): string {
  if ((vk >= 0x41 && vk <= 0x5a) || (vk >= 0x30 && vk <= 0x39)) return String.fromCharCode(vk);
  if (vk >= 0x70 && vk <= 0x87) return `F${vk - 0x6f}`;
  const oem: Record<number, string> = {
    0xba: ";",
    // VK_OEM_PLUS: '+' is the accelerator delimiter — "Ctrl++" has an empty
    // key part and parses back as Ctrl alone. Electron spells it "Plus".
    0xbb: "Plus",
    0xbc: ",",
    0xbd: "-",
    0xbe: ".",
    0xbf: "/",
    0xc0: "`",
    0xdb: "[",
    0xdc: "\\",
    0xdd: "]",
    0xde: "'",
  };
  if (oem[vk]) return oem[vk];
  const named: Record<number, string> = {
    0x08: "Backspace",
    0x09: "Tab",
    0x0d: "Enter",
    0x20: "Space",
    0x21: "PageUp",
    0x22: "PageDown",
    0x23: "End",
    0x24: "Home",
    0x25: "Left",
    0x26: "Up",
    0x27: "Right",
    0x28: "Down",
    0x2d: "Insert",
    0x2e: "Delete",
  };
  return named[vk] ?? "";
}

export function buildAccelerator(payload: {
  vk: number;
  ctrl?: boolean;
  shift?: boolean;
  alt?: boolean;
  win?: boolean;
}): string {
  const parts: string[] = [];
  if (payload.ctrl) parts.push("Ctrl");
  if (payload.alt) parts.push("Alt");
  if (payload.shift) parts.push("Shift");
  if (payload.win) parts.push("Super");
  const key = vkToKeyName(payload.vk);
  if (!key) return "";
  parts.push(key);
  return parts.join("+");
}

function formatTimeSaved(sec: number): { value: string; unit: string } {
  if (sec < 60) return { value: String(Math.max(0, Math.round(sec))), unit: "сек" };
  if (sec < 3600) return { value: String(Math.round(sec / 60)), unit: "мин" };
  return { value: (sec / 3600).toFixed(1), unit: "ч" };
}

function formatTotalWords(n: number): { value: string; unit: string } {
  if (n < 1000) return { value: String(n), unit: "" };
  if (n < 1_000_000) return { value: (n / 1000).toFixed(1), unit: "k" };
  return { value: (n / 1_000_000).toFixed(1), unit: "M" };
}

export function normalizeDictationConfig(
  config?: Partial<DictationConfigData> | null,
): DictationConfigData {
  return {
    ...DEFAULT_DICTATION_CFG,
    ...(config ?? {}),
    networkProfile: config?.networkProfile ?? { kind: "system" },
    localModelPath: config?.localModelPath ?? null,
    localCommandPath: config?.localCommandPath ?? null,
    localModelId: config?.localModelId ?? DEFAULT_DICTATION_CFG.localModelId,
    localEngine: config?.localEngine ?? DEFAULT_DICTATION_CFG.localEngine,
    // null = "никогда не выгружать" — явное значение; undefined (отсутствует) →
    // дефолт 5 минут.
    localIdleUnloadMs:
      config != null && "localIdleUnloadMs" in config
        ? (config.localIdleUnloadMs ?? null)
        : DEFAULT_DICTATION_CFG.localIdleUnloadMs,
  };
}

export function applyDictationConfigSnapshot(
  config: Partial<DictationConfigData> | null,
  dictationConfig: { value: DictationConfigData },
  dictationCustomDohUrl: { value: string },
) {
  const next = normalizeDictationConfig(config);
  dictationConfig.value = next;
  if (next.networkProfile.kind === "custom_doh") {
    dictationCustomDohUrl.value = next.networkProfile.url ?? "";
  }
}

export function normalizeDictationStats(
  stats?: Partial<DictationStatsData> | null,
): DictationStatsData {
  return {
    ...DEFAULT_DICTATION_STATS,
    ...(stats ?? {}),
  };
}

export function buildDictationMicOptions(devices: { deviceId: string; label: string }[]) {
  return [
    { value: "default", label: "Системный по умолчанию" },
    ...devices.map((device) => ({
      value: device.deviceId,
      label: device.label,
    })),
  ];
}

export function buildStatsCards(stats: DictationStatsData) {
  const wpm = Math.round(stats.wpm);
  const saved = formatTimeSaved(stats.timeSavedSeconds);
  const total = formatTotalWords(stats.totalWords);
  return [
    { key: "wpm", label: "WPM", value: String(wpm), unit: "" },
    { key: "saved", label: "Сэкономлено", value: saved.value, unit: saved.unit },
    { key: "words", label: "Всего слов", value: total.value, unit: total.unit },
  ];
}

const DICTATION_GROQ_SPEECH_MODELS = [
  { id: "whisper-large-v3-turbo", label: "Whisper Large V3 Turbo" },
  { id: "whisper-large-v3", label: "Whisper Large V3" },
] as const;

export function buildDictationVoiceModelOptions(args: {
  hasApiKey: boolean;
  localModels: DictationLocalModelsSnapshot | null;
}) {
  const options: DictationVoiceModelOption[] = [];

  if (args.hasApiKey) {
    for (const model of DICTATION_GROQ_SPEECH_MODELS) {
      options.push({
        value: `groq:${model.id}`,
        label: model.label,
        description: "Онлайн-распознавание речи через Groq",
        provider: "groq",
        iconProvider: "groq",
      });
    }
  }

  const localRuntimeReady = Boolean(args.localModels?.commandInstalled);
  if (!localRuntimeReady) {
    return options;
  }
  for (const model of args.localModels?.models ?? []) {
    if (!model.downloaded) continue;
    if (model.transcriptionSupported === false) continue;
    options.push({
      value: `local:${model.id}`,
      label: model.name,
      description: "Локальное распознавание речи",
      provider: "local",
      iconProvider: model.id.includes("parakeet") ? "nvidia" : "openai",
    });
  }

  return options;
}

export function buildDictationVoiceModelValue(
  config: Pick<DictationConfigData, "provider" | "model" | "localModelId">,
): string {
  if (config.provider === "groq") {
    return `groq:${config.model || DEFAULT_DICTATION_CFG.model}`;
  }
  if (config.provider === "local") {
    return `local:${config.localModelId || DEFAULT_DICTATION_CFG.localModelId}`;
  }
  return "";
}

export function resolveAvailableDictationVoiceModelValue(
  config: Pick<DictationConfigData, "provider" | "model" | "localModelId">,
  args: {
    hasApiKey: boolean;
    localModels: DictationLocalModelsSnapshot | null;
  },
): string | null {
  const options = buildDictationVoiceModelOptions(args);
  const current = buildDictationVoiceModelValue(config);
  if (options.some((option) => option.value === current)) {
    return current;
  }
  return options[0]?.value ?? null;
}

export function buildDictationGroqStatus(provider: DictationProvider, hasApiKey: boolean): string {
  if (provider !== "groq") {
    return hasApiKey
      ? "Groq-ключ сохранён, но сейчас выбрана локальная модель."
      : "Groq-ключ не задан.";
  }
  return hasApiKey ? "Groq-ключ установлен." : "Groq-ключ не задан — добавьте его в разделе «AI».";
}

export function buildDictationLocalStatus(
  provider: DictationProvider,
  path: string,
  modelId: string,
): string {
  const trimmed = path.trim();
  if (provider !== "local") {
    return trimmed
      ? `Локальная модель подготовлена: ${modelId}`
      : "Локальный режим ещё не настроен.";
  }
  if (!trimmed) return "Локальная модель не выбрана.";
  return `${modelId} · готово`;
}

export function validateCustomDohUrl(raw: string): string | null {
  const url = raw.trim();
  if (!url) return "URL пустой";
  if (!url.startsWith("https://")) return "URL должен начинаться с https://";
  const rest = url.slice("https://".length);
  if (!rest) return "Хост не задан";
  if (rest.includes("@")) return "userinfo (user:pass@host) не поддерживается";
  const slash = rest.indexOf("/");
  const authority = slash >= 0 ? rest.slice(0, slash) : rest;
  if (!authority) return "Хост не задан";
  return null;
}
