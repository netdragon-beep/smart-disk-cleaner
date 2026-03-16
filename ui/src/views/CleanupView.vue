<script setup lang="ts">
import { ref, computed, h } from "vue";
import { useRouter } from "vue-router";
import {
  NCard,
  NSpace,
  NDataTable,
  NButton,
  NTag,
  NAlert,
  NEmpty,
  NRadioGroup,
  NRadioButton,
  NInput,
  NModal,
  NText,
  NSwitch,
  type DataTableColumns,
  type DataTableRowKey,
} from "naive-ui";
import { useAppStore } from "@/stores/app";
import { useCleanup } from "@/composables/useCleanup";
import type { FileSuggestion } from "@/types";

const TEXT = {
  delete: "\u5220\u9664",
  keep: "\u4FDD\u7559",
  move: "\u79FB\u52A8",
  review: "\u5F85\u5BA1",
  low: "\u4F4E",
  medium: "\u4E2D",
  high: "\u9AD8",
  fileName: "\u6587\u4EF6\u540D",
  action: "\u64CD\u4F5C",
  risk: "\u98CE\u9669",
  reason: "\u539F\u56E0",
  noResult: "\u8FD8\u6CA1\u6709\u626B\u63CF\u7ED3\u679C\uFF0C\u8BF7\u5148\u6267\u884C\u626B\u63CF\u3002",
  goScan: "\u53BB\u626B\u63CF",
  cleanupOptions: "\u6E05\u7406\u9009\u9879",
  recycleBin: "\u56DE\u6536\u7AD9",
  moveLabel: "\u79FB\u52A8",
  targetDir: "\u76EE\u6807\u76EE\u5F55...",
  dryRun: "\u6A21\u62DF\u8FD0\u884C",
  chooseFiles: "\u9009\u62E9\u6587\u4EF6",
  protectedNotice: "\u5176\u4E2D\u90E8\u5206\u6587\u4EF6\u88AB\u6807\u8BB0\u4E3A\u201C\u4FDD\u7559\u201D\u6216\u201C\u5F85\u5BA1\u201D\uFF0C\u5DF2\u7981\u6B62\u5728\u8FD9\u91CC\u76F4\u63A5\u6267\u884C\u6E05\u7406\u3002",
  executeCleanup: "\u6267\u884C\u6E05\u7406",
  selected: "\u5DF2\u9009",
  item: "\u9879",
  error: "\u9519\u8BEF",
  confirmCleanup: "\u786E\u8BA4\u6E05\u7406",
  simulate: "\u6A21\u62DF",
  execute: "\u6267\u884C",
  recycle: "\u56DE\u6536",
  cancel: "\u53D6\u6D88",
  confirmExecute: "\u786E\u8BA4\u6267\u884C",
  simulateRun: "\u6A21\u62DF\u8FD0\u884C",
  fileQuestionSuffix: "\u4E2A\u6587\u4EF6\uFF1F",
  realChangeWarning: "\u8FD9\u5C06\u5BF9\u60A8\u7684\u6587\u4EF6\u7CFB\u7EDF\u8FDB\u884C\u771F\u5B9E\u66F4\u6539\uFF01",
  executionResult: "\u6267\u884C\u7ED3\u679C",
  success: "\u6210\u529F",
  failed: "\u5931\u8D25",
};

const actionLabels: Record<string, string> = {
  delete: TEXT.delete,
  keep: TEXT.keep,
  move: TEXT.moveLabel,
  review: TEXT.review,
};

const riskLabels: Record<string, string> = {
  low: TEXT.low,
  medium: TEXT.medium,
  high: TEXT.high,
};

const router = useRouter();
const store = useAppStore();
const { executing, logs, error, executeCleanup } = useCleanup();

const report = computed(() => store.report);
const mode = ref<"recycle" | "move">("recycle");
const targetDir = ref("");
const dryRun = ref(true);
const showConfirm = ref(false);
const showResults = ref(false);
const selectedPaths = ref<DataTableRowKey[]>([]);

const suggestions = computed(() =>
  (report.value?.advisor.suggestions ?? []).filter(
    (item) => item.action === "delete" || item.action === "move"
  )
);
const protectedSuggestionCount = computed(
  () =>
    (report.value?.advisor.suggestions ?? []).filter(
      (item) => item.action === "keep" || item.action === "review"
    ).length
);

const columns: DataTableColumns<FileSuggestion> = [
  { type: "selection" },
  {
    title: TEXT.fileName,
    key: "path",
    ellipsis: { tooltip: true },
    render: (row) => {
      const parts = row.path.split(/[/\\]/);
      return parts[parts.length - 1];
    },
  },
  {
    title: TEXT.action,
    key: "action",
    width: 90,
    render: (row) =>
      h(
        NTag,
        {
          type:
            row.action === "delete"
              ? "error"
              : row.action === "keep"
                ? "success"
                : row.action === "move"
                  ? "warning"
                  : "info",
          size: "small",
        },
        () => actionLabels[row.action] || row.action
      ),
  },
  {
    title: TEXT.risk,
    key: "risk",
    width: 80,
    render: (row) =>
      h(
        NTag,
        {
          type:
            row.risk === "high"
              ? "error"
              : row.risk === "medium"
                ? "warning"
                : "success",
          size: "small",
        },
        () => riskLabels[row.risk] || row.risk
      ),
  },
  {
    title: TEXT.reason,
    key: "reason",
    ellipsis: { tooltip: true },
  },
];

function rowKey(row: FileSuggestion) {
  return row.path;
}

function handleConfirm() {
  if (selectedPaths.value.length === 0) return;
  showConfirm.value = true;
}

async function handleExecute() {
  showConfirm.value = false;
  const paths = selectedPaths.value.map(String);
  const result = await executeCleanup(
    paths,
    mode.value,
    mode.value === "move" ? targetDir.value || null : null,
    dryRun.value
  );
  if (result.length > 0) {
    store.addHistory(result);
    showResults.value = true;
  }
}
</script>

<template>
  <div v-if="!report">
    <n-empty :description="TEXT.noResult">
      <template #extra>
        <n-button @click="router.push({ name: 'scan' })">{{ TEXT.goScan }}</n-button>
      </template>
    </n-empty>
  </div>

  <div v-else>
    <n-space vertical :size="16">
      <n-card :title="TEXT.cleanupOptions">
        <n-space :size="16" align="center">
          <n-radio-group v-model:value="mode">
            <n-radio-button value="recycle">{{ TEXT.recycleBin }}</n-radio-button>
            <n-radio-button value="move">{{ TEXT.moveLabel }}</n-radio-button>
          </n-radio-group>
          <n-input
            v-if="mode === 'move'"
            v-model:value="targetDir"
            :placeholder="TEXT.targetDir"
            style="width: 300px"
          />
          <n-space align="center">
            <n-text>{{ TEXT.dryRun }}</n-text>
            <n-switch v-model:value="dryRun" />
          </n-space>
        </n-space>
      </n-card>

      <n-card :title="TEXT.chooseFiles">
        <n-alert v-if="protectedSuggestionCount > 0" type="info" style="margin-bottom: 12px">
          {{ TEXT.protectedNotice }}
        </n-alert>
        <n-data-table
          :columns="columns"
          :data="suggestions"
          :row-key="rowKey"
          :max-height="400"
          size="small"
          :bordered="false"
          v-model:checked-row-keys="selectedPaths"
        />
        <template #action>
          <n-button
            type="primary"
            :disabled="selectedPaths.length === 0 || executing"
            :loading="executing"
            @click="handleConfirm"
          >
            {{ TEXT.executeCleanup }} ({{ TEXT.selected }} {{ selectedPaths.length }} {{ TEXT.item }})
          </n-button>
        </template>
      </n-card>

      <n-alert v-if="error" type="error" :title="TEXT.error">
        {{ error }}
      </n-alert>
    </n-space>

    <n-modal
      v-model:show="showConfirm"
      preset="dialog"
      :title="TEXT.confirmCleanup"
      :positive-text="dryRun ? TEXT.simulateRun : TEXT.confirmExecute"
      :negative-text="TEXT.cancel"
      @positive-click="handleExecute"
      :type="dryRun ? 'info' : 'warning'"
    >
      <n-text>
        {{ dryRun ? TEXT.simulate : TEXT.execute }}
        {{ mode === 'recycle' ? TEXT.recycle : TEXT.moveLabel }}
        {{ selectedPaths.length }} {{ TEXT.fileQuestionSuffix }}
      </n-text>
      <n-alert v-if="!dryRun" type="warning" style="margin-top: 12px">
        {{ TEXT.realChangeWarning }}
      </n-alert>
    </n-modal>

    <n-modal v-model:show="showResults" preset="card" :title="TEXT.executionResult" style="width: 600px">
      <n-space vertical :size="8">
        <div v-for="(entry, idx) in logs" :key="idx">
          <n-tag :type="entry.success ? 'success' : 'error'" size="small">
            {{ entry.success ? TEXT.success : TEXT.failed }}
          </n-tag>
          <n-text style="margin-left: 8px; font-size: 13px">
            {{ entry.path.split(/[/\\]/).pop() }}: {{ entry.detail }}
          </n-text>
        </div>
      </n-space>
    </n-modal>
  </div>
</template>
