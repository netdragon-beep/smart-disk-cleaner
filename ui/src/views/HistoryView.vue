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
  time: "\u65F6\u95F4",
  fileName: "\u6587\u4EF6\u540D",
  mode: "\u6A21\u5F0F",
  status: "\u72B6\u6001",
  success: "\u6210\u529F",
  failed: "\u5931\u8D25",
  dryRun: "\u6A21\u62DF",
  yes: "\u662F",
  no: "\u5426",
  detail: "\u8BE6\u60C5",
  operationHistory: "\u64CD\u4F5C\u5386\u53F2",
  noHistory: "\u6682\u65E0\u64CD\u4F5C\u5386\u53F2\u8BB0\u5F55\u3002",
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
