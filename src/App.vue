<script setup lang="ts">
import {
  Button,
  SettingsButtonRow,
  SettingsDropdownRow,
  SettingsList,
  SettingsTextInputRow,
  Skeleton,
} from "@kosmos/visuals";
import { computed, onMounted, ref } from "vue";

type Config = {
  hotkey: string;
  language: string;
  injectMode: "auto_paste" | "clipboard_only";
  provider: "groq" | "local";
  model: string;
  providerEnabled: boolean;
};

type EngineResult<T> = { ok: true; data: T } | { ok: false; message: string };

const fallback: Config = {
  hotkey: "Ctrl+Shift+;",
  language: "ru",
  injectMode: "auto_paste",
  provider: "groq",
  model: "whisper-large-v3-turbo",
  providerEnabled: true,
};
const config = ref<Config>({ ...fallback });
const loading = ref(true);
const saving = ref(false);
const message = ref("");
const hasApiKey = ref(false);

const providerLabel = computed(() => (config.value.provider === "local" ? "Локальная" : "Groq"));
const injectOptions = [
  { value: "auto_paste", label: "Вставлять автоматически" },
  { value: "clipboard_only", label: "Только копировать" },
] as const;
const providerOptions = [
  { value: "groq", label: "Groq" },
  { value: "local", label: "Локальная модель" },
] as const;

const request = async <T,>(operation: string, params: Record<string, unknown> = {}): Promise<T> => {
  if (!window.kosmosApp?.ark) throw new Error("Откройте Dictation из Kosmos");
  const result = await window.kosmosApp.ark.request<EngineResult<T>>(operation, params);
  if (!result.ok) throw new Error(result.message);
  return result.data;
};

async function load() {
  loading.value = true;
  try {
    const result = await request<{ config?: Partial<Config>; hasApiKey?: boolean }>("dictation.get_config");
    config.value = { ...fallback, ...result.config };
    hasApiKey.value = result.hasApiKey === true;
  } catch (error) {
    message.value = error instanceof Error ? error.message : "Не удалось загрузить настройки";
  } finally {
    loading.value = false;
  }
}

async function save() {
  saving.value = true;
  message.value = "";
  try {
    await request("dictation.update_config", config.value as unknown as Record<string, unknown>);
    message.value = "Настройки сохранены";
  } catch (error) {
    message.value = error instanceof Error ? error.message : "Не удалось сохранить";
  } finally {
    saving.value = false;
  }
}

async function toggleRecording() {
  try {
    const state = await request<{ state?: string }>("dictation.get_state");
    await request(state.state === "recording" ? "dictation.cancel" : "dictation.start_recording");
    message.value = state.state === "recording" ? "Диктовка остановлена" : "Диктовка запущена";
  } catch (error) {
    message.value = error instanceof Error ? error.message : "Не удалось изменить состояние диктовки";
  }
}

onMounted(load);
</script>

<template>
  <main class="dictation-settings">
    <header class="dictation-header">
      <div>
        <h1>Настройки Dictation</h1>
        <p>Горячая клавиша, распознавание и вставка текста.</p>
      </div>
      <Button variant="surface" size="sm" @click="toggleRecording">Начать диктовку</Button>
    </header>

    <p v-if="message" class="dictation-message" role="status">{{ message }}</p>
    <div v-if="loading" class="dictation-loading">
      <Skeleton class="h-56 w-full" />
      <Skeleton class="h-28 w-full" />
    </div>
    <template v-else>
      <SettingsList>
        <SettingsTextInputRow
          v-model="config.hotkey"
          title="Горячая клавиша"
          description="Работает глобально, пока запущен Kosmos Desktop."
          placeholder="Ctrl+Shift+;"
        />
        <SettingsTextInputRow
          v-model="config.language"
          title="Язык"
          description="Код языка распознавания."
          placeholder="ru"
        />
        <SettingsDropdownRow
          v-model="config.injectMode"
          title="После распознавания"
          :options="injectOptions"
          :searchable="false"
        />
        <SettingsDropdownRow
          v-model="config.provider"
          title="Провайдер"
          :options="providerOptions"
          :searchable="false"
        />
        <SettingsTextInputRow v-model="config.model" title="Модель" />
        <SettingsButtonRow
          title="Сохранить настройки"
          description="Новая горячая клавиша применяется сразу."
          button-label="Сохранить"
          variant="surface"
          :loading="saving"
          @click="save"
        />
      </SettingsList>

      <SettingsList>
        <SettingsButtonRow
          title="Ключ Groq"
          :description="hasApiKey ? 'Ключ сохранён в защищённом хранилище Kosmos.' : 'Добавьте ключ на странице «Ключи» в Kosmos Manager.'"
          button-label="В Kosmos Manager"
          variant="surface"
          disabled
        />
        <SettingsButtonRow
          :title="providerLabel"
          description="Микрофон, горячая клавиша и вставка текста выполняются Kosmos Desktop."
          button-label="Активно"
          variant="surface"
          disabled
          muted
        />
      </SettingsList>
    </template>
  </main>
</template>
