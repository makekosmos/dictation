export type DnsKind = "system" | "cloudflare_doh" | "google_doh" | "custom_doh";
export type DictationProvider = "groq" | "mock" | "local";

export interface DictationConfigData {
  hotkey: string;
  triggerMode: "toggle" | "push_to_talk";
  language: string;
  injectMode: "auto_paste" | "clipboard_only";
  networkProfile: { kind: DnsKind; url?: string };
  httpProxy: string | null;
  transcriptionPrompt: string;
  provider: DictationProvider;
  providerEnabled: boolean;
  model: string;
  localModelPath: string | null;
  localCommandPath: string | null;
  localModelId: string;
  localEngine: string;
  microphoneDeviceId: string | null;
  duckAudioDuringRecording: boolean;
  /** Через сколько мс простоя выгружать whisper-server. null = никогда. */
  localIdleUnloadMs: number | null;
}

interface ConnectivityStage {
  name: "client_build" | "dns_resolve" | "tcp_connect" | "http_head";
  ok: boolean;
  ms: number;
  error?: string;
  ip?: string;
  status?: number;
}

export interface ConnectivityReport {
  ok: boolean;
  totalMs: number;
  stages: ConnectivityStage[];
  firstFailure: string | null;
}

export interface DictationStatsData {
  totalWords: number;
  totalRecordSeconds: number;
  totalSessions: number;
  wpm: number;
  timeSavedSeconds: number;
}

interface DictationLocalModelInfo {
  id: string;
  name: string;
  description: string;
  filename: string;
  url: string;
  sizeMb: number;
  accuracyScore: number;
  speedScore: number;
  recommended: boolean;
  transcriptionSupported?: boolean;
  directory?: boolean;
  downloaded: boolean;
  selected: boolean;
  path: string | null;
}

export interface DictationLocalModelsSnapshot {
  modelsDir: string;
  commandPath: string | null;
  commandInstalled: boolean;
  models: DictationLocalModelInfo[];
}

export interface DictationVoiceModelOption {
  value: string;
  label: string;
  description?: string;
  disabled?: boolean;
  provider?: DictationProvider;
  iconProvider?: "groq" | "openai" | "nvidia" | "speech" | "text";
}

export const DEFAULT_DICTATION_CFG: DictationConfigData = {
  hotkey: "Ctrl+Shift+;",
  triggerMode: "toggle",
  language: "ru",
  injectMode: "auto_paste",
  networkProfile: { kind: "system" },
  httpProxy: null,
  transcriptionPrompt: "",
  provider: "groq",
  providerEnabled: true,
  model: "whisper-large-v3",
  localModelPath: null,
  localCommandPath: null,
  localModelId: "whisper-large-v3",
  localEngine: "whisper.cpp",
  microphoneDeviceId: null,
  duckAudioDuringRecording: false,
  localIdleUnloadMs: 300000,
};

export const DICTATION_IDLE_UNLOAD_OPTIONS = [
  { value: 30000, label: "30 сек" },
  { value: 60000, label: "1 минута" },
  { value: 180000, label: "3 минуты" },
  { value: 300000, label: "5 минут" },
  { value: null, label: "Не выгружать" },
] as const;

export const DEFAULT_DICTATION_STATS: DictationStatsData = {
  totalWords: 0,
  totalRecordSeconds: 0,
  totalSessions: 0,
  wpm: 0,
  timeSavedSeconds: 0,
};

export const DNS_PROFILE_OPTIONS = [
  { value: "system", label: "Системный" },
  { value: "cloudflare_doh", label: "Cloudflare (1.1.1.1)" },
  { value: "google_doh", label: "Google (8.8.8.8)" },
  { value: "custom_doh", label: "Свой DoH URL" },
] as const;

export const DICTATION_TRIGGER_OPTIONS = [
  {
    value: "toggle",
    label: "Toggle (двойное нажатие)",
    description: "Первое нажатие — старт, второе — отправка.",
  },
  {
    value: "push_to_talk",
    label: "Push-to-talk (удерживать)",
    description: "Удерживайте клавишу пока говорите.",
  },
] as const;

export const DICTATION_INJECT_OPTIONS = [
  {
    value: "auto_paste",
    label: "Auto-paste",
    description: "Симулирует Ctrl+V и восстанавливает буфер.",
  },
  {
    value: "clipboard_only",
    label: "Только в буфер обмена",
    description: "Текст записывается в буфер, Ctrl+V — вручную.",
  },
] as const;

export const DICTATION_LANGUAGE_OPTIONS = [
  { value: "auto", label: "Авто" },
  { value: "ru", label: "Русский" },
  { value: "en", label: "English" },
  { value: "uk", label: "Українська" },
  { value: "be", label: "Беларуская" },
  { value: "de", label: "Deutsch" },
  { value: "fr", label: "Français" },
  { value: "es", label: "Español" },
  { value: "it", label: "Italiano" },
  { value: "pt", label: "Português" },
  { value: "pl", label: "Polski" },
  { value: "tr", label: "Türkçe" },
  { value: "ja", label: "日本語" },
  { value: "ko", label: "한국어" },
  { value: "zh", label: "中文" },
  { value: "ar", label: "العربية" },
  { value: "he", label: "עברית" },
  { value: "hi", label: "हिन्दी" },
  { value: "nl", label: "Nederlands" },
  { value: "sv", label: "Svenska" },
  { value: "fi", label: "Suomi" },
  { value: "cs", label: "Čeština" },
  { value: "el", label: "Ελληνικά" },
] as const;

export const DICTATION_PROVIDER_OPTIONS = [
  { value: "groq", label: "Groq Cloud" },
  { value: "local", label: "Локальная модель" },
] as const;
