<script setup lang="ts">
import { computed, onMounted, ref } from "vue";

type Config = {
  hotkey: string;
  language: string;
  injectMode: "auto_paste" | "clipboard_only";
  provider: "groq" | "local";
  model: string;
  providerEnabled: boolean;
};

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
type EngineResult<T> = { ok: true; data: T } | { ok: false; message: string };

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
    message.value = "Сохранено";
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
  <main class="app">
    <header class="header">
      <div>
        <p class="eyebrow">Kosmos</p>
        <h1>Dictation</h1>
        <p>Голосовой ввод без доступа к вашим данным ARK.</p>
      </div>
      <button class="primary" type="button" @click="toggleRecording">Начать диктовку</button>
    </header>

    <p v-if="message" class="dictation-message">{{ message }}</p>
    <section v-if="loading" class="dictation-card">Загрузка…</section>
    <template v-else>
      <section class="dictation-card">
        <h2>Основное</h2>
        <label>Горячая клавиша <input v-model="config.hotkey" placeholder="Ctrl+Shift+;" /></label>
        <label>Язык <input v-model="config.language" placeholder="ru" /></label>
        <label>Вставка
          <select v-model="config.injectMode"><option value="auto_paste">Вставлять автоматически</option><option value="clipboard_only">Только скопировать</option></select>
        </label>
        <label>Провайдер
          <select v-model="config.provider"><option value="groq">Groq</option><option value="local">Локальная модель</option></select>
        </label>
        <label>Модель <input v-model="config.model" /></label>
        <button class="primary" type="button" :disabled="saving" @click="save">{{ saving ? "Сохранение…" : "Сохранить" }}</button>
      </section>

      <section class="dictation-card">
        <h2>Ключ Groq</h2>
        <p>{{ hasApiKey ? "Ключ сохранён в Kosmos Manager." : "Ключ пока не задан." }}</p>
        <p>Добавьте или замените его в Kosmos Manager → Secrets. Dictation не получает значение ключа.</p>
      </section>

      <section class="dictation-card dictation-muted">
        <h2>{{ providerLabel }}</h2>
        <p>Микрофон, глобальная клавиша, overlay и вставка текста выполняются Kosmos Desktop. Этот продукт не запрашивает доступ к объектам, синхронизации или данным ARK.</p>
      </section>
    </template>
  </main>
</template>
