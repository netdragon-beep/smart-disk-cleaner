<script setup lang="ts">
import { ref, onMounted, h } from "vue";
import {
  NCard,
  NDataTable,
  NTag,
  NEmpty,
  type DataTableColumns,
} from "naive-ui";
import { useCleanup } from "@/composables/useCleanup";
import { useAppStore } from "@/stores/app";
import type { OperationLogEntry } from "@/types";

const TEXT = {
  recycle: "\u56DE\u6536",
  move: "\u79FB\u52A8",
  registry: "\u6CE8\u518C\u8868",
  migration: "\u8FC1\u79FB",
  cleanup: "\u6587\u4EF6\u6CBB\u7406",
  time: "\u65F6\u95F4",
  fileName: "\u6587\u4EF6\u540D",
  mode: "\u6A21\u5F0F",
  category: "\u7C7B\u578B",
  status: "\u72B6\u6001",
  success: "\u6210\u529F",
  failed: "\u5931\u8D25",
  dryRun: "\u6A21\u62DF",
  yes: "\u662F",
  no: "\u5426",
  reasonLabel: "\u539F\u56E0",
  rollbackRef: "\u56DE\u6EDA\u5F15\u7528",
  detail: "\u8BE6\u60C5",
  operationHistory: "\u6CBB\u7406\u5386\u53F2",
  noHistory: "\u6682\u65E0\u6CBB\u7406\u5386\u53F2\u8BB0\u5F55\u3002",
};

const store = useAppStore();
const { getHistory } = useCleanup();
const loading = ref(false);

onMounted(async () => {
  loading.value = true;
  const entries = await getHistory();
  store.setHistory(entries);
  loading.value = false;
});

const modeLabels: Record<string, string> = {
  recycle: TEXT.recycle,
  move: TEXT.move,
};

const categoryLabels: Record<string, string> = {
  file_cleanup: TEXT.cleanup,
  migration: TEXT.migration,
  registry_change: TEXT.registry,
  registry_rollback: TEXT.registry,
};

const columns: DataTableColumns<OperationLogEntry> = [
  {
    title: TEXT.time,
    key: "at",
    width: 170,
    render: (row) => new Date(row.at).toLocaleString(),
    sorter: (a, b) => new Date(a.at).getTime() - new Date(b.at).getTime(),
    defaultSortOrder: "descend",
  },
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
    title: TEXT.mode,
    key: "mode",
    width: 90,
    render: (row) => h(NTag, { size: "small" }, () => modeLabels[row.mode] || row.mode),
  },
  {
    title: TEXT.category,
    key: "recordKind",
    width: 110,
    render: (row) =>
      h(
        NTag,
        { size: "small", type: row.recordKind.startsWith("registry") ? "warning" : "info" },
        () => categoryLabels[row.recordKind] || row.recordKind
      ),
  },
  {
    title: TEXT.status,
    key: "success",
    width: 90,
    render: (row) =>
      h(
        NTag,
        { type: row.success ? "success" : "error", size: "small" },
        () => (row.success ? TEXT.success : TEXT.failed)
      ),
  },
  {
    title: TEXT.dryRun,
    key: "dryRun",
    width: 80,
    render: (row) =>
      h(NTag, { size: "small", type: row.dryRun ? "info" : "default" }, () =>
        row.dryRun ? TEXT.yes : TEXT.no
      ),
  },
  {
    title: TEXT.detail,
    key: "detail",
    ellipsis: { tooltip: true },
  },
  {
    title: TEXT.reasonLabel,
    key: "reason",
    ellipsis: { tooltip: true },
    render: (row) => row.reason || "-",
  },
  {
    title: TEXT.rollbackRef,
    key: "rollbackReference",
    ellipsis: { tooltip: true },
    render: (row) => row.rollbackReference || "-",
  },
];
</script>

<template>
  <n-card :title="TEXT.operationHistory">
    <n-data-table
      v-if="store.history.length > 0"
      :columns="columns"
      :data="store.history"
      :max-height="500"
      size="small"
      :bordered="false"
      :loading="loading"
    />
    <n-empty v-else :description="TEXT.noHistory" />
  </n-card>
</template>
