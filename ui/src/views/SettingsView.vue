<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import {
  NAlert,
  NButton,
  NCard,
  NDynamicTags,
  NForm,
  NFormItem,
  NInput,
  NInputNumber,
  NSelect,
  NSpace,
  NSwitch,
  useMessage,
} from "naive-ui";
import { useConfig } from "@/composables/useConfig";
import { useAppStore } from "@/stores/app";
import type { AppConfig } from "@/types";

const TEXT = {
  saved: "\u8BBE\u7F6E\u5DF2\u4FDD\u5B58\uFF01",
  saveFailed: "\u4FDD\u5B58\u8BBE\u7F6E\u5931\u8D25",
  settings: "\u8BBE\u7F6E",
  aiHint:
    "\u914D\u7F6E API URL\u3001Key \u548C\u6A21\u578B\u540E\uFF0C\u626B\u63CF\u65F6\u4F1A\u8C03\u7528 AI \u53C2\u4E0E\u6E05\u7406\u5EFA\u8BAE\u751F\u6210\u3002AI \u53EA\u80FD\u5728\u672C\u5730\u5B89\u5168\u89C4\u5219\u5141\u8BB8\u7684\u8303\u56F4\u5185\u4FEE\u6B63\u5EFA\u8BAE\u3002",
  largeFileThreshold: "\u5927\u6587\u4EF6\u9608\u503C (MB)",
  maxAiItems: "AI \u5019\u9009\u6761\u76EE\u6570",
  apiKey: "API Key",
  apiKeyPlaceholder: "\u8F93\u5165 OpenAI \u6216\u517C\u5BB9 API \u7684 Key",
  aiBaseUrl: "API URL",
  aiBaseUrlPlaceholder: "https://api.openai.com",
  aiModel: "\u6A21\u578B\u9009\u62E9",
  aiModelPreset: "\u9884\u7F6E\u6A21\u578B",
  aiModelCustom: "\u81EA\u5B9A\u4E49\u6A21\u578B",
  aiModelCustomPlaceholder: "\u4F8B\u5982 gpt-4.1-mini \u6216\u5176\u4ED6\u517C\u5BB9\u6A21\u578B\u540D",
  aiModelHint:
    "\u5982\u679C\u4F60\u7684\u63A5\u53E3\u63D0\u4F9B\u5546\u4F7F\u7528\u81EA\u5B9A\u4E49\u6A21\u578B\u540D\u79F0\uFF0C\u8BF7\u5728\u4E0B\u65B9\u76F4\u63A5\u586B\u5199\u3002",
  aiTest: "AI \u8FDE\u63A5\u6D4B\u8BD5",
  aiTestButton: "\u6D4B\u8BD5 AI \u8FDE\u63A5",
  aiTestSuccess: "AI \u8FDE\u63A5\u6210\u529F",
  aiTestFailed: "AI \u8FDE\u63A5\u5931\u8D25",
  excludePatterns: "\u6392\u9664\u89C4\u5219",
  darkMode: "\u6DF1\u8272\u6A21\u5F0F",
  saveSettings: "\u4FDD\u5B58\u8BBE\u7F6E",
};

const COMMON_MODELS = [
  "gpt-4.1-mini",
  "gpt-4.1",
  "gpt-4o-mini",
  "gpt-4o",
  "o4-mini",
];

const store = useAppStore();
const { loadConfig, saveConfig, testAiConfig } = useConfig();
const message = useMessage();

const form = ref<AppConfig>({
  largeFileThresholdMb: 512,
  maxAiItems: 20,
  apiKey: null,
  aiBaseUrl: "https://api.openai.com",
  aiModel: "gpt-4.1-mini",
  excludePatterns: [],
  theme: "dark",
});

const saving = ref(false);
const testingAi = ref(false);
const selectedPresetModel = ref<string | null>(null);
const customModelValue = ref("");

const modelOptions = computed(() => {
  return COMMON_MODELS.map((value) => ({
    label: value,
    value,
  }));
});

const canTestAi = computed(
  () =>
    Boolean(form.value.aiBaseUrl.trim()) &&
    Boolean(form.value.aiModel.trim()) &&
    Boolean(form.value.apiKey?.trim())
);

onMounted(async () => {
  const cfg = await loadConfig();
  if (cfg) {
    form.value = { ...cfg };
  }
  syncModelControls();
});

watch(
  () => form.value.aiModel,
  () => {
    syncModelControls();
  }
);

async function handleSave() {
  saving.value = true;
  const ok = await saveConfig(form.value);
  if (ok) {
    store.setConfig(form.value);
    message.success(TEXT.saved);
  } else {
    message.error(TEXT.saveFailed);
  }
  saving.value = false;
}

async function handleTestAi() {
  testingAi.value = true;
  const result = await testAiConfig(form.value);
  if (result) {
    message.success(`${TEXT.aiTestSuccess}：${result}`);
  } else {
    message.error(TEXT.aiTestFailed);
  }
  testingAi.value = false;
}

function toggleTheme() {
  form.value.theme = form.value.theme === "dark" ? "light" : "dark";
}

function syncModelControls() {
  if (COMMON_MODELS.includes(form.value.aiModel)) {
    selectedPresetModel.value = form.value.aiModel;
    customModelValue.value = "";
    return;
  }
  selectedPresetModel.value = null;
  customModelValue.value = form.value.aiModel;
}

function handlePresetModelUpdate(value: string | null) {
  selectedPresetModel.value = value;
  if (value) {
    form.value.aiModel = value;
  }
}

function handleCustomModelUpdate(value: string) {
  customModelValue.value = value;
  if (value.trim()) {
    form.value.aiModel = value.trim();
    selectedPresetModel.value = null;
  }
}
</script>

<template>
  <div style="max-width: 760px; margin: 0 auto">
    <n-card :title="TEXT.settings">
      <n-form label-placement="left" label-width="180" :model="form">
        <n-alert type="info" style="margin-bottom: 16px">
          {{ TEXT.aiHint }}
        </n-alert>

        <n-form-item :label="TEXT.largeFileThreshold">
          <n-input-number
            v-model:value="form.largeFileThresholdMb"
            :min="1"
            :max="10240"
            style="width: 100%"
          />
        </n-form-item>

        <n-form-item :label="TEXT.maxAiItems">
          <n-input-number
            v-model:value="form.maxAiItems"
            :min="1"
            :max="100"
            style="width: 100%"
          />
        </n-form-item>

        <n-form-item :label="TEXT.apiKey">
          <n-input
            v-model:value="form.apiKey"
            type="password"
            show-password-on="click"
            :placeholder="TEXT.apiKeyPlaceholder"
            style="width: 100%"
          />
        </n-form-item>

        <n-form-item :label="TEXT.aiBaseUrl">
          <n-input
            v-model:value="form.aiBaseUrl"
            :placeholder="TEXT.aiBaseUrlPlaceholder"
            style="width: 100%"
          />
        </n-form-item>

        <n-form-item :label="TEXT.aiModel">
          <n-space vertical :size="8" style="width: 100%">
            <n-select
              :value="selectedPresetModel"
              :options="modelOptions"
              clearable
              style="width: 100%"
              @update:value="handlePresetModelUpdate"
            />
            <n-input
              :value="customModelValue"
              :placeholder="TEXT.aiModelCustomPlaceholder"
              style="width: 100%"
              @update:value="handleCustomModelUpdate"
            />
            <n-alert type="info">
              {{ TEXT.aiModelHint }}
            </n-alert>
          </n-space>
        </n-form-item>

        <n-form-item :label="TEXT.aiTest">
          <n-button
            @click="handleTestAi"
            :disabled="!canTestAi"
            :loading="testingAi"
          >
            {{ TEXT.aiTestButton }}
          </n-button>
        </n-form-item>

        <n-form-item :label="TEXT.excludePatterns">
          <n-dynamic-tags v-model:value="form.excludePatterns" />
        </n-form-item>

        <n-form-item :label="TEXT.darkMode">
          <n-switch
            :value="form.theme === 'dark'"
            @update:value="toggleTheme"
          />
        </n-form-item>
      </n-form>

      <template #action>
        <n-button
          type="primary"
          :loading="saving"
          @click="handleSave"
          style="width: 100%"
        >
          {{ TEXT.saveSettings }}
        </n-button>
      </template>
    </n-card>
  </div>
</template>
