<script setup lang="ts">
import { computed } from "vue";
import { useRouter } from "vue-router";
import {
  NCard,
  NSpace,
  NStatistic,
  NGrid,
  NGi,
  NDataTable,
  NTag,
  NButton,
  NText,
  NEmpty,
  type DataTableColumns,
} from "naive-ui";
import VChart from "vue-echarts";
import { use } from "echarts/core";
import { PieChart } from "echarts/charts";
import {
  TitleComponent,
  TooltipComponent,
  LegendComponent,
} from "echarts/components";
import { CanvasRenderer } from "echarts/renderers";
import { useAppStore } from "@/stores/app";
import type { FileRecord, FileSuggestion, SuggestedAction } from "@/types";

const TEXT = {
  localRules: "\u672C\u5730\u89C4\u5219\u5206\u6790",
  remoteModel: "\u8FDC\u7A0B AI \u6A21\u578B\uFF1A",
  fileType: "\u6587\u4EF6\u7C7B\u578B",
  noExt: "\u65E0\u6269\u5C55\u540D",
  fileName: "\u6587\u4EF6\u540D",
  size: "\u5927\u5C0F",
  ext: "\u6269\u5C55\u540D",
  noScanResult: "\u8FD8\u6CA1\u6709\u626B\u63CF\u7ED3\u679C\u3002",
  goScan: "\u53BB\u626B\u63CF",
  overview: "\u6982\u89C8",
  totalFiles: "\u6587\u4EF6\u603B\u6570",
  totalSize: "\u603B\u5927\u5C0F",
  duplicateGroups: "\u91CD\u590D\u6587\u4EF6\u7EC4",
  suggestionCount: "\u5EFA\u8BAE\u6570",
  typeDistribution: "\u6587\u4EF6\u7C7B\u578B\u5206\u5E03",
  largeFiles: "\u5927\u6587\u4EF6\u5217\u8868",
  keep: "\u4FDD\u7559",
  review: "\u5F85\u5BA1",
  deleteAdvice: "\u5EFA\u8BAE\u5220\u9664",
  moveAdvice: "\u5EFA\u8BAE\u79FB\u52A8",
  duplicate: "\u91CD\u590D",
  aiSummary: "AI \u6458\u8981",
  goCleanup: "\u524D\u5F80\u6E05\u7406",
  groupPrefix: "\u7B2C",
  groupSuffix: "\u7EC4",
  suggestionsLimited: "\u5EFA\u8BAE\u5217\u8868\u5DF2\u622A\u65AD\uFF0C\u4EC5\u5C55\u793A\u524D 1000 \u6761\u3002",
  duplicateGroupsLimited: "\u91CD\u590D\u6587\u4EF6\u7EC4\u5DF2\u622A\u65AD\uFF0C\u4EC5\u5C55\u793A\u524D 10 \u7EC4\u3002",
};

use([PieChart, TitleComponent, TooltipComponent, LegendComponent, CanvasRenderer]);

const router = useRouter();
const store = useAppStore();
const report = computed(() => store.report);
const duplicateGroupCount = computed(
  () => report.value?.dedup.groupCount ?? report.value?.dedup.groups.length ?? 0
);
const suggestionCount = computed(
  () => report.value?.advisor.suggestionCount ?? report.value?.advisor.suggestions.length ?? 0
);

const suggestionByPath = computed(() => {
  const map = new Map<string, FileSuggestion>();
  for (const item of report.value?.advisor.suggestions ?? []) {
    map.set(item.path, item);
  }
  return map;
});

const advisorSourceLabel = computed(() => {
  const source = report.value?.advisor.source;
  if (!source) return "";
  if (source === "local_rules") return TEXT.localRules;
  if (source.startsWith("remote:")) {
    return `${TEXT.remoteModel}${source.slice("remote:".length)}`;
  }
  return source;
});

function formatBytes(bytes: number): string {
  if (bytes < 1024) return bytes + " B";
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + " KB";
  if (bytes < 1024 * 1024 * 1024)
    return (bytes / (1024 * 1024)).toFixed(1) + " MB";
  return (bytes / (1024 * 1024 * 1024)).toFixed(2) + " GB";
}

const typeChartOption = computed(() => {
  if (!report.value) return {};
  const breakdown = report.value.analysis.typeBreakdown.slice(0, 10);
  return {
    tooltip: { trigger: "item", formatter: "{b}: {d}%" },
    legend: { orient: "vertical", right: 10, top: 20 },
    series: [
      {
        name: TEXT.fileType,
        type: "pie",
        radius: ["40%", "70%"],
        avoidLabelOverlap: false,
        itemStyle: { borderRadius: 6, borderColor: "#fff", borderWidth: 2 },
        label: { show: false },
        emphasis: { label: { show: true, fontSize: 14, fontWeight: "bold" } },
        data: breakdown.map((t) => ({
          name: t.extension || TEXT.noExt,
          value: t.totalSize,
        })),
      },
    ],
  };
});

const largeFileColumns: DataTableColumns<FileRecord> = [
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
    title: TEXT.size,
    key: "size",
    width: 120,
    sorter: (a, b) => a.size - b.size,
    render: (row) => formatBytes(row.size),
  },
  {
    title: TEXT.ext,
    key: "extension",
    width: 100,
    render: (row) => row.extension || "-",
  },
];

function goToCleanup() {
  router.push({ name: "cleanup" });
}

function duplicateTagType(path: string): "default" | "success" | "warning" | "error" | "info" {
  const action = suggestionByPath.value.get(path)?.action;
  if (action === "keep") return "success";
  if (action === "review") return "info";
  if (action === "move") return "warning";
  if (action === "delete") return "error";
  return "default";
}

function duplicateTagLabel(path: string): string {
  const action = suggestionByPath.value.get(path)?.action as SuggestedAction | undefined;
  if (action === "keep") return TEXT.keep;
  if (action === "review") return TEXT.review;
  if (action === "move") return TEXT.moveAdvice;
  if (action === "delete") return TEXT.deleteAdvice;
  return TEXT.duplicate;
}
</script>

<template>
  <div v-if="!report">
    <n-empty :description="TEXT.noScanResult">
      <template #extra>
        <n-button @click="router.push({ name: 'scan' })">{{ TEXT.goScan }}</n-button>
      </template>
    </n-empty>
  </div>

  <div v-else>
    <n-space vertical :size="20">
      <n-card :title="TEXT.overview">
        <n-grid :cols="4" :x-gap="12">
          <n-gi>
            <n-statistic :label="TEXT.totalFiles" :value="report.analysis.totalFiles" />
          </n-gi>
          <n-gi>
            <n-statistic :label="TEXT.totalSize" :value="formatBytes(report.analysis.totalSize)" />
          </n-gi>
          <n-gi>
            <n-statistic :label="TEXT.duplicateGroups" :value="duplicateGroupCount" />
          </n-gi>
          <n-gi>
            <n-statistic :label="TEXT.suggestionCount" :value="suggestionCount" />
          </n-gi>
        </n-grid>
      </n-card>

      <n-card :title="TEXT.typeDistribution">
        <v-chart :option="typeChartOption" style="height: 300px" autoresize />
      </n-card>

      <n-card :title="TEXT.largeFiles">
        <n-data-table
          :columns="largeFileColumns"
          :data="report.analysis.largeFiles.slice(0, 50)"
          :max-height="300"
          size="small"
          :bordered="false"
        />
      </n-card>

      <n-card :title="TEXT.duplicateGroups" v-if="report.dedup.groups.length > 0">
        <n-space vertical :size="12">
          <n-text v-if="report.dedup.truncated" depth="3">
            {{ TEXT.duplicateGroupsLimited }}
          </n-text>
          <n-card
            v-for="(group, idx) in report.dedup.groups"
            :key="group.hash"
            :title="`${TEXT.groupPrefix} ${idx + 1} ${TEXT.groupSuffix} (${formatBytes(group.totalSize)})`"
            size="small"
          >
            <n-space vertical :size="4">
              <div v-for="file in group.files" :key="file.path">
                <n-tag
                  :type="duplicateTagType(file.path)"
                  size="small"
                >
                  {{ duplicateTagLabel(file.path) }}
                </n-tag>
                <n-text style="margin-left: 8px; font-size: 13px">
                  {{ file.path }}
                </n-text>
              </div>
            </n-space>
          </n-card>
        </n-space>
      </n-card>

      <n-card :title="TEXT.aiSummary">
        <n-tag :type="report.advisor.source.startsWith('remote') ? 'info' : 'default'" size="small">
          {{ advisorSourceLabel }}
        </n-tag>
        <n-text v-if="report.advisor.truncated" depth="3" style="display: block; margin-top: 12px">
          {{ TEXT.suggestionsLimited }}
        </n-text>
        <n-text style="display: block; margin-top: 12px; white-space: pre-wrap">
          {{ report.advisor.summary }}
        </n-text>
      </n-card>

      <n-button type="primary" @click="goToCleanup" style="width: 100%">
        {{ TEXT.goCleanup }}
      </n-button>
    </n-space>
  </div>
</template>
