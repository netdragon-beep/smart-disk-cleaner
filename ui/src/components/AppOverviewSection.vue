<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { onMounted, ref, watch } from "vue";
import { NAlert, NButton, NCard, NEmpty, NGi, NGrid, NSpace, NTag, NText } from "naive-ui";
import type { AppOverviewRow } from "@/types";

const props = defineProps<{
  reportKey: string;
  selectedAppKey?: string | null;
}>();

const emit = defineEmits<{
  (e: "select-app", value: AppOverviewRow): void;
  (e: "clear-app"): void;
}>();

const TEXT = {
  title: "应用分类概览",
  hint:
    "把扫描结果按软件目录聚合展示。这里更适合回答“这台电脑装了哪些应用、它们分别占了多少空间”。",
  loading: "正在识别应用目录...",
  loadFailed: "应用分类加载失败",
  empty: "当前扫描结果中还没有识别到可展示的应用目录。",
  root: "识别根目录",
  samplePath: "样本路径",
  source: "来源",
  selected: "已选中",
  clear: "清除筛选",
  files: "个文件",
  unknownVendor: "未知厂商",
  unknownCategory: "未分类",
  defaultStatus: "已识别",
  expand: "展开详情",
  collapse: "收起详情",
} as const;

const rows = ref<AppOverviewRow[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);
const expandedKeys = ref<string[]>([]);
let requestId = 0;

watch(
  () => props.reportKey,
  async (key) => {
    if (!key) {
      rows.value = [];
      error.value = null;
      loading.value = false;
      expandedKeys.value = [];
      return;
    }
    await loadOverview();
  },
  { immediate: true }
);

onMounted(async () => {
  if (props.reportKey) {
    await loadOverview();
  }
});

async function loadOverview() {
  const currentId = ++requestId;
  loading.value = true;
  error.value = null;

  try {
    const result = await invoke<AppOverviewRow[]>("get_app_overview_v2");
    if (currentId !== requestId) return;
    rows.value = result;
  } catch (reason) {
    if (currentId !== requestId) return;
    rows.value = [];
    error.value = typeof reason === "string" ? reason : (reason as Error).message || String(reason);
  } finally {
    if (currentId === requestId) {
      loading.value = false;
    }
  }
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function appStatusTags(item: AppOverviewRow): string[] {
  return item.statusTags.length > 0 ? item.statusTags : [TEXT.defaultStatus];
}

function isSelected(item: AppOverviewRow) {
  return props.selectedAppKey === item.key;
}

function isExpanded(item: AppOverviewRow) {
  return expandedKeys.value.includes(item.key);
}

function handleClick(item: AppOverviewRow) {
  if (isSelected(item)) {
    emit("clear-app");
    return;
  }
  emit("select-app", item);
}

function toggleExpanded(item: AppOverviewRow) {
  if (isExpanded(item)) {
    expandedKeys.value = expandedKeys.value.filter((key) => key !== item.key);
    return;
  }
  expandedKeys.value = [...expandedKeys.value, item.key];
}
</script>

<template>
  <n-card class="surface-card interactive-card">
    <template #header>
      <div class="section-head">
        <div>
          <div class="section-head__title">{{ TEXT.title }}</div>
          <div class="section-head__desc">{{ TEXT.hint }}</div>
        </div>
        <n-button v-if="selectedAppKey" size="small" secondary @click="emit('clear-app')">
          {{ TEXT.clear }}
        </n-button>
      </div>
    </template>

    <n-space vertical :size="14">
      <n-alert v-if="error" type="error" :title="TEXT.loadFailed">
        {{ error }}
      </n-alert>

      <div v-else-if="loading" class="soft-panel">
        <n-text depth="3">{{ TEXT.loading }}</n-text>
      </div>

      <n-grid
        v-else-if="rows.length > 0"
        cols="1 s:2 l:2"
        responsive="screen"
        :x-gap="14"
        :y-gap="14"
      >
        <n-gi v-for="item in rows" :key="item.key">
          <div
            class="app-overview-card interactive-card"
            :class="{ 'app-overview-card--selected': isSelected(item) }"
            @click="handleClick(item)"
          >
            <div class="app-overview-card__top">
              <div class="app-overview-card__identity">
                <img
                  :src="item.iconDataUri"
                  :alt="item.appName"
                  class="app-overview-card__icon"
                />
                <div class="app-overview-card__meta">
                  <div class="app-overview-card__title">{{ item.appName }}</div>
                  <div class="app-overview-card__subtitle">
                    {{ item.vendor || TEXT.unknownVendor }} · {{ item.category || TEXT.unknownCategory }}
                  </div>
                  <div class="app-overview-card__root-preview">
                    {{ item.detectedRoot }}
                  </div>
                </div>
              </div>

              <n-tag v-if="isSelected(item)" size="small" type="success" round>
                {{ TEXT.selected }}
              </n-tag>
            </div>

            <div class="app-overview-card__tags">
              <n-tag size="small" round>{{ TEXT.source }}：{{ item.sourceSummary }}</n-tag>
              <n-tag
                v-for="tag in appStatusTags(item)"
                :key="`${item.key}-${tag}`"
                size="small"
                type="info"
                round
              >
                {{ tag }}
              </n-tag>
              <n-tag size="small" type="success" round>{{ formatBytes(item.totalSize) }}</n-tag>
              <n-tag size="small" round>{{ item.fileCount }} {{ TEXT.files }}</n-tag>
            </div>

            <div v-if="isExpanded(item)" class="soft-panel app-overview-card__body">
              <div class="app-overview-card__field">
                <span class="app-overview-card__label">{{ TEXT.root }}</span>
                <span class="app-overview-card__value">{{ item.detectedRoot }}</span>
              </div>

              <div v-if="item.samplePaths.length > 0" class="app-overview-card__field">
                <span class="app-overview-card__label">{{ TEXT.samplePath }}</span>
                <span
                  v-for="sample in item.samplePaths"
                  :key="sample"
                  class="app-overview-card__value app-overview-card__sample"
                >
                  {{ sample }}
                </span>
              </div>
            </div>

            <div class="app-overview-card__footer">
              <n-button size="small" tertiary @click.stop="toggleExpanded(item)">
                {{ isExpanded(item) ? TEXT.collapse : TEXT.expand }}
              </n-button>
            </div>
          </div>
        </n-gi>
      </n-grid>

      <n-empty v-else :description="TEXT.empty" />
    </n-space>
  </n-card>
</template>

<style scoped>
.app-overview-card {
  display: flex;
  flex-direction: column;
  gap: 12px;
  height: 100%;
  padding: 16px;
  border-radius: 22px;
  border: 1px solid var(--border-soft);
  background: linear-gradient(135deg, #ffffff 0%, #fafcff 100%);
  cursor: pointer;
}

.app-overview-card--selected {
  border-color: rgba(23, 133, 108, 0.42);
  box-shadow:
    0 18px 40px rgba(23, 133, 108, 0.12),
    inset 0 0 0 1px rgba(23, 133, 108, 0.18);
}

.app-overview-card__top {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  align-items: flex-start;
}

.app-overview-card__identity {
  display: flex;
  gap: 12px;
  min-width: 0;
  align-items: center;
}

.app-overview-card__icon {
  flex: 0 0 44px;
  width: 44px;
  height: 44px;
  border-radius: 14px;
  object-fit: cover;
  background: #fff;
  border: 1px solid var(--border-soft);
}

.app-overview-card__meta {
  min-width: 0;
}

.app-overview-card__title {
  font-size: 16px;
  font-weight: 800;
  color: var(--text-strong);
}

.app-overview-card__subtitle {
  margin-top: 4px;
  color: var(--text-normal);
  line-height: 1.5;
}

.app-overview-card__root-preview {
  margin-top: 6px;
  color: var(--text-soft);
  line-height: 1.45;
  word-break: break-all;
  display: -webkit-box;
  overflow: hidden;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
}

.app-overview-card__tags {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.app-overview-card__body {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.app-overview-card__field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.app-overview-card__label {
  font-size: 12px;
  font-weight: 700;
  color: var(--text-soft);
}

.app-overview-card__value {
  line-height: 1.7;
  word-break: break-all;
  color: var(--text-normal);
}

.app-overview-card__sample + .app-overview-card__sample {
  margin-top: 4px;
}

.app-overview-card__footer {
  display: flex;
  justify-content: flex-end;
}
</style>
