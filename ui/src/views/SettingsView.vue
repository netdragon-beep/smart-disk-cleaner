<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import {
  NAlert,
  NButton,
  NCard,
  NDynamicTags,
  NForm,
  NFormItem,
  NInput,
  NInputNumber,
  NSpace,
  NSwitch,
  useMessage,
} from "naive-ui";
import { useConfig } from "@/composables/useConfig";
import { useAppStore } from "@/stores/app";
import type { AppConfig } from "@/types";

const TEXT = {
  saved: "设置已保存！",
  saveFailed: "保存设置失败",
  settings: "设置",
  aiHint:
    "配置 API URL、Key 和模型后，扫描时会调用 AI 参与清理建议生成。AI 只能在本地安全规则允许的范围内修正建议。",
  largeFileThreshold: "大文件阈值 (MB)",
  maxAiItems: "AI 候选条目数",
  apiKey: "API Key",
  apiKeyPlaceholder: "输入 OpenAI 或兼容 API 的 Key",
  aiBaseUrl: "API URL",
  aiBaseUrlPlaceholder: "https://api.openai.com 或 https://your-provider/v1",
  aiModel: "模型选择",
  aiModelCustomPlaceholder: "例如 gpt-4.1-mini 或供应商自定义模型名",
  aiModelHint:
    "优先从当前 API 提供商动态拉取 /models。若供应商使用自定义模型名，也可以直接在下方手动输入。",
  aiModelLoadedPrefix: "已从当前 API 提供商加载 ",
  aiModelLoadedSuffix: " 个模型，可直接从下拉框选择。",
  loadModelsButton: "加载模型列表",
  loadModelsSuccess: "模型列表加载成功",
  loadModelsFailed: "模型列表加载失败",
  aiTest: "AI 连接测试",
  aiTestButton: "测试 AI 连接",
  aiTestSuccess: "AI 连接成功",
  aiTestFailed: "AI 连接失败",
  strictFileAiRemoteOnly: "路径 AI 严格远程模式",
  strictFileAiRemoteOnlyHint:
    "开启后，文件或目录的 AI 解读都必须成功调用远程模型；如果 API Key 未配置、超时、401 或返回格式不兼容，将直接报错，不再静默回退到本地规则。",
  excludePatterns: "排除规则",
  darkMode: "深色模式",
  saveSettings: "保存设置",
};

const FALLBACK_MODELS = [
  "gpt-4.1-mini",
  "gpt-4.1",
  "gpt-4o-mini",
  "gpt-4o",
  "o4-mini",
];

const store = useAppStore();
const { loadConfig, saveConfig, testAiConfig, listAiModels, error } = useConfig();
const message = useMessage();

const form = ref<AppConfig>({
  largeFileThresholdMb: 512,
  maxAiItems: 20,
  apiKey: null,
  aiBaseUrl: "https://api.openai.com",
  aiModel: "gpt-4.1-mini",
  strictFileAiRemoteOnly: false,
  excludePatterns: [],
  theme: "dark",
});

const saving = ref(false);
const testingAi = ref(false);
const loadingModels = ref(false);
const providerModels = ref<string[]>([]);
const lastLoadedSignature = ref("");
let refreshModelsTimer: ReturnType<typeof setTimeout> | null = null;

const canTestAi = computed(
  () =>
    Boolean(form.value.aiBaseUrl.trim()) &&
    Boolean(form.value.aiModel.trim()) &&
    Boolean(form.value.apiKey?.trim())
);

const canLoadModels = computed(
  () => Boolean(form.value.aiBaseUrl.trim()) && Boolean(form.value.apiKey?.trim())
);

const modelHintText = computed(() => {
  if (providerModels.value.length > 0) {
    return `${TEXT.aiModelLoadedPrefix}${providerModels.value.length}${TEXT.aiModelLoadedSuffix}`;
  }
  return TEXT.aiModelHint;
});

const suggestedModels = computed(() => {
  return providerModels.value.length > 0 ? providerModels.value : FALLBACK_MODELS;
});

onMounted(async () => {
  const cfg = await loadConfig();
  if (cfg) {
    form.value = { ...cfg };
    store.setConfig({ ...cfg });
  }
  await loadModelOptions({ silent: true });
});

onUnmounted(() => {
  if (refreshModelsTimer !== null) {
    clearTimeout(refreshModelsTimer);
  }
});

watch(
  () => [form.value.aiBaseUrl, form.value.apiKey],
  () => {
    providerModels.value = [];
    lastLoadedSignature.value = "";
    if (refreshModelsTimer !== null) {
      clearTimeout(refreshModelsTimer);
    }
    if (!canLoadModels.value) {
      return;
    }
    refreshModelsTimer = setTimeout(() => {
      void loadModelOptions({ silent: true });
    }, 500);
  }
);

watch(
  form,
  (value) => {
    store.setConfig({ ...value });
  },
  { deep: true }
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

async function handleLoadModels() {
  await loadModelOptions({ force: true });
}

function toggleTheme() {
  form.value.theme = form.value.theme === "dark" ? "light" : "dark";
}

function handleModelUpdate(value: string) {
  form.value.aiModel = value;
}

function applySuggestedModel(value: string) {
  form.value.aiModel = value;
}

function buildModelsSignature() {
  return `${form.value.aiBaseUrl.trim()}\n${form.value.apiKey?.trim() ?? ""}`;
}

async function loadModelOptions(options: { silent?: boolean; force?: boolean } = {}) {
  const { silent = false, force = false } = options;
  if (!canLoadModels.value) {
    providerModels.value = [];
    return;
  }

  const signature = buildModelsSignature();
  if (!force && signature === lastLoadedSignature.value && providerModels.value.length > 0) {
    return;
  }

  loadingModels.value = true;
  const models = await listAiModels(form.value);
  if (models && models.length > 0) {
    providerModels.value = models;
    lastLoadedSignature.value = signature;
    if (!silent) {
      message.success(`${TEXT.loadModelsSuccess}（${models.length}）`);
    }
  } else if (!silent) {
    message.error(error.value || TEXT.loadModelsFailed);
  }
  loadingModels.value = false;
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
            <n-input
              :value="form.aiModel"
              :placeholder="TEXT.aiModelCustomPlaceholder"
              style="width: 100%"
              @update:value="handleModelUpdate"
            />
            <n-button
              secondary
              :loading="loadingModels"
              :disabled="!canLoadModels"
              @click="handleLoadModels"
            >
              {{ TEXT.loadModelsButton }}
            </n-button>
            <n-space v-if="suggestedModels.length > 0" wrap :size="[8, 8]">
              <n-button
                v-for="model in suggestedModels"
                :key="model"
                size="small"
                secondary
                :type="form.aiModel === model ? 'primary' : 'default'"
                @click="applySuggestedModel(model)"
              >
                {{ model }}
              </n-button>
            </n-space>
            <n-alert :type="providerModels.length > 0 ? 'success' : 'info'">
              {{ modelHintText }}
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

        <n-form-item :label="TEXT.strictFileAiRemoteOnly">
          <n-space vertical :size="8" style="width: 100%">
            <n-switch v-model:value="form.strictFileAiRemoteOnly" />
            <n-alert type="warning">
              {{ TEXT.strictFileAiRemoteOnlyHint }}
            </n-alert>
          </n-space>
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
