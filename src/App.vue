<script setup lang="ts">
import {
  DesktopChrome,
  SettingsButtonRow,
  SettingsDropdownRow,
  SettingsList,
  SettingsSidebar,
  SettingsSidebarButton,
  SettingsTextInputRow,
  Skeleton,
} from "@kosmos/visuals";
import { Settings2 } from "@lucide/vue";
import { computed, onMounted, ref } from "vue";
import groqIconMarkup from "./assets/providers/groq.svg?raw";
import kosmosIconMarkup from "./assets/providers/kosmos.svg?raw";
import { createDictationApi, type DictationConfig } from "./lib/dictationApi";

type Config = DictationConfig;

const fallback: Config = {
  hotkey: "Ctrl+Shift+;",
  language: "ru",
  injectMode: "auto_paste",
  provider: "groq",
  model: "whisper-large-v3-turbo",
  localModelId: "small",
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
const modelOptions = [
  {
    value: "groq:whisper-large-v3-turbo",
    label: "Whisper Large V3 Turbo",
    description: "Онлайн-распознавание через Groq",
  },
  {
    value: "groq:whisper-large-v3",
    label: "Whisper Large V3",
    description: "Онлайн-распознавание через Groq",
  },
  {
    value: "local:tiny-q5_1",
    label: "Whisper Tiny",
    description: "Быстрая локальная модель",
  },
  {
    value: "local:small",
    label: "Whisper Small",
    description: "Локальная модель для повседневной диктовки",
  },
  {
    value: "local:medium",
    label: "Whisper Medium",
    description: "Более высокое качество, но тяжелее для CPU",
  },
  {
    value: "local:turbo",
    label: "Whisper Large V3 Turbo",
    description: "Локальный баланс качества и скорости",
  },
  {
    value: "local:large",
    label: "Whisper Large V3 q5",
    description: "Максимальное качество из локального списка",
  },
  {
    value: "local:parakeet-tdt-0.6b-v3",
    label: "Parakeet V3",
    description: "Быстрая локальная модель NVIDIA",
  },
] as const;

const selectedModel = computed(
  () =>
    `${config.value.provider}:${config.value.provider === "local" ? config.value.localModelId : config.value.model}`,
);

const dictation = createDictationApi(window.kosmosApp?.ark);

function requireDictation() {
  if (!dictation) throw new Error("Откройте Dictation из Kosmos");
  return dictation;
}

function selectModel(value: string) {
  const [provider, model] = value.split(":", 2);
  if (!model || (provider !== "groq" && provider !== "local")) return;
  config.value.provider = provider;
  config.value.providerEnabled = true;
  if (provider === "local") {
    config.value.localModelId = model;
  } else {
    config.value.model = model;
  }
}

async function load() {
  loading.value = true;
  try {
    const result = await requireDictation().getConfig();
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
    await requireDictation().updateConfig(config.value);
    message.value = "Настройки сохранены";
  } catch (error) {
    message.value = error instanceof Error ? error.message : "Не удалось сохранить";
  } finally {
    saving.value = false;
  }
}

onMounted(load);
</script>

<template>
  <DesktopChrome appearance="settings" platform="windows">
    <template #sidebar>
      <SettingsSidebar title="Kosmos" background="var(--bg-app)">
        <div class="dictation-sidebar-scroll kosmos-scroll">
          <div class="dictation-sidebar-group">
            <SettingsSidebarButton :icon="Settings2" label="Настройки" active />
          </div>
        </div>
      </SettingsSidebar>
    </template>

    <template #titlebar-center>
      <span class="dictation-titlebar-title">Dictation</span>
    </template>

    <main class="dictation-settings">
      <header class="dictation-header">
        <div>
          <h1>Настройки</h1>
          <p>Горячая клавиша, распознавание и вставка текста.</p>
        </div>
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
            :model-value="selectedModel"
            title="Модель"
            description="Провайдер и модель распознавания."
            :options="modelOptions"
            searchable
            search-placeholder="Поиск модели"
            @update:model-value="selectModel"
          >
            <template #trigger-leading="{ option }">
              <span v-if="option" class="dictation-model-option-icon" aria-hidden="true">
                <!-- eslint-disable-next-line vue/no-v-html -- bundled SVG asset, not user input. -->
                <span
                  v-html="option.value.startsWith('groq:') ? groqIconMarkup : kosmosIconMarkup"
                />
              </span>
            </template>
            <template #option-leading="{ option }">
              <span class="dictation-model-option-icon" aria-hidden="true">
                <!-- eslint-disable-next-line vue/no-v-html -- bundled SVG asset, not user input. -->
                <span
                  v-html="option.value.startsWith('groq:') ? groqIconMarkup : kosmosIconMarkup"
                />
              </span>
            </template>
          </SettingsDropdownRow>
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
            :description="
              hasApiKey
                ? 'Ключ сохранён в защищённом хранилище Kosmos.'
                : 'Добавьте ключ на странице «Ключи» в Kosmos Manager.'
            "
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
  </DesktopChrome>
</template>
