<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { computed, h, onBeforeUnmount, ref, watch } from "vue";
import { useRouter } from "vue-router";
import {
  NAlert,
  NButton,
  NCard,
  NDataTable,
  NEmpty,
  NGi,
  NGrid,
  NInput,
  NModal,
  NSelect,
  NSpace,
  NStatistic,
  NTag,
  NText,
  type DataTableColumns,
} from "naive-ui";
import VChart from "vue-echarts";
import { use } from "echarts/core";
import { PieChart } from "echarts/charts";
import {
  LegendComponent,
  TitleComponent,
  TooltipComponent,
} from "echarts/components";
import { CanvasRenderer } from "echarts/renderers";
import { useAppStore } from "@/stores/app";
import { useAiFile } from "@/composables/useAiFile";
import type {
  DirectoryOverviewRow,
  FileAiInsight,
  FileRecord,
  FileSuggestion,
  FileTreeQueryResult,
  FileTreeRow,
  ScanModuleKind,
  ScanModuleSummary,
  SuggestedAction,
} from "@/types";

type FileCategory =
  | "all"
  | "image"
  | "video"
  | "audio"
  | "archive"
  | "executable"
  | "document"
  | "code"
  | "other";

const TEXT = {
  localRules: "本地规则分析",
  remoteModel: "远程 AI 模型：",
  fileType: "文件类型",
  noExt: "无扩展名",
  filePath: "路径",
  size: "大小",
  ext: "扩展名",
  noScanResult: "还没有扫描结果。",
  goScan: "去扫描",
  overview: "概览",
  rootPath: "扫描根目录",
  totalFiles: "文件总数",
  totalSize: "总大小",
  duplicateGroups: "重复文件组",
  suggestionCount: "建议数",
  typeDistribution: "文件类型分布",
  scanModules: "扫描模块",
  directoryOverview: "目录概览",
  itemName: "名称",
  itemType: "类型",
  fileCount: "文件数",
  contentPreview: "内容预览",
  directory: "目录",
  file: "文件",
  scannedFiles: "分层文件清单",
  scannedFilesHintPrefix: "共匹配 ",
  scannedFilesHintMiddle: " 个文件，当前展示 ",
  scannedFilesHintSuffix: " 个树节点。",
  fileSearchPlaceholder: "按文件名、路径或扩展名筛选，例如 pdf、src、README",
  fileCategoryPlaceholder: "按文件类型筛选",
  categoryAll: "全部文件",
  categoryImage: "图片文件",
  categoryVideo: "视频文件",
  categoryAudio: "音频文件",
  categoryArchive: "压缩包 / 镜像",
  categoryExecutable: "可执行 / 安装 / 脚本",
  categoryDocument: "文档文件",
  categoryCode: "代码 / 配置",
  categoryOther: "其他文件",
  largeFiles: "大文件",
  temporaryFiles: "临时文件",
  archiveFiles: "压缩包 / 安装包",
  emptyFiles: "空文件",
  emptyDirs: "空目录",
  keep: "保留",
  review: "待审",
  deleteAdvice: "建议删除",
  moveAdvice: "建议移动",
  duplicate: "重复",
  aiSummary: "AI 摘要",
  aiInspect: "AI 解读",
  aiInspectTitle: "路径 AI 解读",
  aiInspectHint:
    "支持对单个文件或目录调用 AI。目录分析只会上传文件名、扩展名、大小、数量等摘要信息，不会读取整个目录下所有文件内容，用来减少 token 消耗。",
  aiInspectFailed: "路径 AI 解读失败",
  aiInspectRemoteSuccess: "已成功调用远程 AI 模型",
  aiInspectFallbackTitle: "远程 AI 调用失败，已回退本地规则",
  aiInspectLocalOnlyTitle: "当前未调用远程 AI，展示的是本地规则结果",
  aiInspectFallbackReason: "回退原因",
  aiInspectReason: "处理建议说明",
  aiInspectSummary: "AI 解读结论",
  aiInspectLoading: "正在分析这个路径，请稍候...",
  aiInspectTargetFile: "文件",
  aiInspectTargetDirectory: "目录",
  aiRiskLow: "低风险",
  aiRiskMedium: "中风险",
  aiRiskHigh: "高风险",
  close: "关闭",
  retry: "重新分析",
  goCleanup: "前往清理",
  groupPrefix: "第",
  groupSuffix: "组",
  duplicateFiles: "重复文件",
  moduleDescDuplicate: "同内容文件组，适合人工确认后清理冗余副本",
  moduleDescLarge: "占用空间高的文件，优先人工评估",
  moduleDescTemporary: "中间态、缓存态或下载残留文件",
  moduleDescArchive: "压缩包、镜像和安装包，适合归档或复核",
  moduleDescEmptyFiles: "体积为 0 的文件",
  moduleDescEmptyDirs: "不包含任何内容的目录",
  emptyOverview: "当前扫描结果没有可展示的目录内容。",
  loadingDirectoryOverview: "正在加载目录概览...",
  loadingFileTree: "正在按当前筛选条件加载文件树...",
  directoryOverviewFailed: "目录概览加载失败",
  fileTreeFailed: "文件清单加载失败",
  fileTreeEmpty: "当前筛选条件下没有匹配文件。",
  fileTreeTruncated: "匹配结果过多，当前仅展示前 3000 个匹配文件以避免页面卡死。",
  fileTreeCollapsed: "结果较多，目录默认折叠显示，避免界面卡顿。",
  suggestionsLimited: "建议列表已做截断，仅展示前 1000 条建议。",
  duplicateGroupsLimited: "重复文件组已做截断，仅展示前 10 组。",
  sectionLimited: "当前结果页仅展示前 50 项，避免大盘扫描导致页面占用过高。",
};

use([PieChart, TitleComponent, TooltipComponent, LegendComponent, CanvasRenderer]);

const router = useRouter();
const store = useAppStore();
const { loading: aiLoading, error: aiError, explainFile } = useAiFile();
const report = computed(() => store.report);
const reportKey = computed(() =>
  report.value ? `${report.value.generatedAt}:${report.value.root}` : ""
);

const fileQuery = ref("");
const selectedCategory = ref<FileCategory>("all");
const aiInsightVisible = ref(false);
const aiInsight = ref<FileAiInsight | null>(null);
const selectedAiPath = ref("");

const directoryOverviewRows = ref<DirectoryOverviewRow[]>([]);
const directoryOverviewLoading = ref(false);
const directoryOverviewError = ref<string | null>(null);

const fileTreeResult = ref<FileTreeQueryResult>(emptyFileTreeResult());
const fileTreeLoading = ref(false);
const fileTreeError = ref<string | null>(null);

let fileTreeTimer: ReturnType<typeof setTimeout> | null = null;
let fileTreeRequestId = 0;
let directoryOverviewRequestId = 0;

const suggestionByPath = computed(() => {
  const map = new Map<string, FileSuggestion>();
  for (const item of report.value?.advisor.suggestions ?? []) {
    map.set(item.path, item);
  }
  return map;
});

const advisorSourceLabel = computed(() => formatSourceLabel(report.value?.advisor.source));
const duplicateGroupCount = computed(
  () => report.value?.dedup.groupCount ?? report.value?.dedup.groups.length ?? 0
);
const suggestionCount = computed(
  () => report.value?.advisor.suggestionCount ?? report.value?.advisor.suggestions.length ?? 0
);

const moduleCards = computed(() =>
  (report.value?.modules ?? []).map((item: ScanModuleSummary) => ({
    ...item,
    label: moduleLabel(item.kind),
    description: moduleDescription(item.kind),
  }))
);

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
        data: breakdown.map((item) => ({
          name: item.extension || TEXT.noExt,
          value: item.totalSize,
        })),
      },
    ],
  };
});

const fileCategoryOptions = [
  { label: TEXT.categoryAll, value: "all" },
  { label: TEXT.categoryImage, value: "image" },
  { label: TEXT.categoryVideo, value: "video" },
  { label: TEXT.categoryAudio, value: "audio" },
  { label: TEXT.categoryArchive, value: "archive" },
  { label: TEXT.categoryExecutable, value: "executable" },
  { label: TEXT.categoryDocument, value: "document" },
  { label: TEXT.categoryCode, value: "code" },
  { label: TEXT.categoryOther, value: "other" },
];

const fileTreeRows = computed(() => fileTreeResult.value.rows);
const scannedFilesHint = computed(() => {
  if (!report.value) return "";
  return `${TEXT.scannedFilesHintPrefix}${fileTreeResult.value.matchedCount}${TEXT.scannedFilesHintMiddle}${fileTreeResult.value.nodeCount}${TEXT.scannedFilesHintSuffix}`;
});
const shouldExpandFileTree = computed(() => fileTreeResult.value.nodeCount <= 200);

const fileTreeColumns: DataTableColumns<FileTreeRow> = [
  {
    title: TEXT.itemName,
    key: "name",
    ellipsis: { tooltip: true },
    render: (row) =>
      h("div", { style: "display: flex; flex-direction: column; gap: 2px;" }, [
        h("div", { style: "display: flex; align-items: center; gap: 8px;" }, [
          h(
            NTag,
            {
              size: "small",
              type: row.kind === "directory" ? "info" : "default",
            },
            () => (row.kind === "directory" ? TEXT.directory : TEXT.file)
          ),
          h("span", row.name),
        ]),
        h(
          NText,
          {
            depth: 3,
            style: "font-size: 12px;",
          },
          () => row.path
        ),
      ]),
  },
  {
    title: TEXT.size,
    key: "size",
    width: 120,
    sorter: (left, right) => left.size - right.size,
    render: (row) => formatBytes(row.size),
  },
  {
    title: TEXT.ext,
    key: "extension",
    width: 110,
    render: (row) => (row.kind === "directory" ? "-" : row.extension || "-"),
  },
  {
    title: TEXT.fileCount,
    key: "fileCount",
    width: 90,
    render: (row) => (row.kind === "directory" ? row.fileCount : "-"),
  },
  {
    title: TEXT.aiInspect,
    key: "aiInspect",
    width: 110,
    render: (row) =>
      h(
        NButton,
        {
          size: "tiny",
          secondary: true,
          type: "primary",
          loading: aiLoading.value && selectedAiPath.value === row.path,
          disabled: aiLoading.value && selectedAiPath.value !== row.path,
          onClick: () => void inspectFileWithAi(row.path),
        },
        () => TEXT.aiInspect
      ),
  },
];

const fileColumns: DataTableColumns<FileRecord> = [
  {
    title: TEXT.filePath,
    key: "path",
    ellipsis: { tooltip: true },
  },
  {
    title: TEXT.size,
    key: "size",
    width: 120,
    sorter: (left, right) => left.size - right.size,
    render: (row) => formatBytes(row.size),
  },
  {
    title: TEXT.ext,
    key: "extension",
    width: 110,
    render: (row) => row.extension || "-",
  },
  {
    title: TEXT.aiInspect,
    key: "aiInspect",
    width: 110,
    render: (row) =>
      h(
        NButton,
        {
          size: "tiny",
          secondary: true,
          type: "primary",
          loading: aiLoading.value && selectedAiPath.value === row.path,
          disabled: aiLoading.value && selectedAiPath.value !== row.path,
          onClick: () => void inspectFileWithAi(row.path),
        },
        () => TEXT.aiInspect
      ),
  },
];

const directoryColumns: DataTableColumns<DirectoryOverviewRow> = [
  {
    title: TEXT.itemName,
    key: "name",
    ellipsis: { tooltip: true },
  },
  {
    title: TEXT.itemType,
    key: "kind",
    width: 90,
    render: (row) =>
      h(
        NTag,
        {
          size: "small",
          type: row.kind === "directory" ? "info" : "default",
        },
        () => (row.kind === "directory" ? TEXT.directory : TEXT.file)
      ),
  },
  {
    title: TEXT.fileCount,
    key: "fileCount",
    width: 90,
  },
  {
    title: TEXT.size,
    key: "totalSize",
    width: 120,
    render: (row) => formatBytes(row.totalSize),
  },
  {
    title: TEXT.contentPreview,
    key: "preview",
    ellipsis: { tooltip: true },
  },
  {
    title: TEXT.aiInspect,
    key: "aiInspect",
    width: 110,
    render: (row) =>
      h(
        NButton,
        {
          size: "tiny",
          secondary: true,
          type: "primary",
          loading: aiLoading.value && selectedAiPath.value === row.path,
          disabled: aiLoading.value && selectedAiPath.value !== row.path,
          onClick: () => void inspectFileWithAi(row.path),
        },
        () => TEXT.aiInspect
      ),
  },
];

watch(
  reportKey,
  async (key) => {
    if (!key) {
      resetDirectoryOverviewState();
      resetFileTreeState();
      return;
    }

    await loadDirectoryOverview();
    scheduleFileTreeLoad(0);
  },
  { immediate: true }
);

watch([fileQuery, selectedCategory], () => {
  if (!reportKey.value) {
    return;
  }
  scheduleFileTreeLoad(250);
});

onBeforeUnmount(() => {
  if (fileTreeTimer) {
    clearTimeout(fileTreeTimer);
    fileTreeTimer = null;
  }
});

function emptyFileTreeResult(): FileTreeQueryResult {
  return {
    matchedCount: 0,
    nodeCount: 0,
    truncated: false,
    rows: [],
  };
}

function resetDirectoryOverviewState() {
  directoryOverviewRows.value = [];
  directoryOverviewLoading.value = false;
  directoryOverviewError.value = null;
}

function resetFileTreeState() {
  fileTreeResult.value = emptyFileTreeResult();
  fileTreeLoading.value = false;
  fileTreeError.value = null;
}

function scheduleFileTreeLoad(delayMs: number) {
  if (fileTreeTimer) {
    clearTimeout(fileTreeTimer);
  }
  fileTreeTimer = setTimeout(() => {
    fileTreeTimer = null;
    void loadFileTree();
  }, delayMs);
}

async function loadDirectoryOverview() {
  const requestId = ++directoryOverviewRequestId;
  directoryOverviewLoading.value = true;
  directoryOverviewError.value = null;

  try {
    const result = await invoke<DirectoryOverviewRow[]>("get_directory_overview");
    if (requestId !== directoryOverviewRequestId) {
      return;
    }
    directoryOverviewRows.value = result;
  } catch (error) {
    if (requestId !== directoryOverviewRequestId) {
      return;
    }
    directoryOverviewRows.value = [];
    directoryOverviewError.value =
      typeof error === "string" ? error : (error as Error).message || String(error);
  } finally {
    if (requestId === directoryOverviewRequestId) {
      directoryOverviewLoading.value = false;
    }
  }
}

async function loadFileTree() {
  const requestId = ++fileTreeRequestId;
  fileTreeLoading.value = true;
  fileTreeError.value = null;

  try {
    const result = await invoke<FileTreeQueryResult>("query_file_tree", {
      query: fileQuery.value.trim() || null,
      category: selectedCategory.value,
    });
    if (requestId !== fileTreeRequestId) {
      return;
    }
    fileTreeResult.value = result;
  } catch (error) {
    if (requestId !== fileTreeRequestId) {
      return;
    }
    fileTreeResult.value = emptyFileTreeResult();
    fileTreeError.value =
      typeof error === "string" ? error : (error as Error).message || String(error);
  } finally {
    if (requestId === fileTreeRequestId) {
      fileTreeLoading.value = false;
    }
  }
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function moduleLabel(kind: ScanModuleKind): string {
  if (kind === "duplicate_files") return TEXT.duplicateFiles;
  if (kind === "large_files") return TEXT.largeFiles;
  if (kind === "temporary_files") return TEXT.temporaryFiles;
  if (kind === "archive_files") return TEXT.archiveFiles;
  if (kind === "empty_files") return TEXT.emptyFiles;
  return TEXT.emptyDirs;
}

function moduleDescription(kind: ScanModuleKind): string {
  if (kind === "duplicate_files") return TEXT.moduleDescDuplicate;
  if (kind === "large_files") return TEXT.moduleDescLarge;
  if (kind === "temporary_files") return TEXT.moduleDescTemporary;
  if (kind === "archive_files") return TEXT.moduleDescArchive;
  if (kind === "empty_files") return TEXT.moduleDescEmptyFiles;
  return TEXT.moduleDescEmptyDirs;
}

function goToCleanup() {
  router.push({ name: "cleanup" });
}

async function inspectFileWithAi(path: string) {
  selectedAiPath.value = path;
  aiInsightVisible.value = true;
  aiInsight.value = null;
  const result = await explainFile(path, store.config);
  if (result) {
    aiInsight.value = result;
  }
}

function formatSourceLabel(source?: string | null): string {
  if (!source) return "";
  if (source === "local_rules") return TEXT.localRules;
  if (source.startsWith("remote:")) {
    return `${TEXT.remoteModel}${source.slice("remote:".length)}`;
  }
  return source;
}

function actionTagType(
  action: SuggestedAction
): "default" | "success" | "warning" | "error" | "info" {
  if (action === "keep") return "success";
  if (action === "review") return "info";
  if (action === "move") return "warning";
  if (action === "delete") return "error";
  return "default";
}

function actionLabel(action: SuggestedAction): string {
  if (action === "keep") return TEXT.keep;
  if (action === "review") return TEXT.review;
  if (action === "move") return TEXT.moveAdvice;
  if (action === "delete") return TEXT.deleteAdvice;
  return action;
}

function riskTagType(risk: "low" | "medium" | "high"): "success" | "warning" | "error" {
  if (risk === "low") return "success";
  if (risk === "medium") return "warning";
  return "error";
}

function riskLabel(risk: "low" | "medium" | "high"): string {
  if (risk === "low") return TEXT.aiRiskLow;
  if (risk === "medium") return TEXT.aiRiskMedium;
  return TEXT.aiRiskHigh;
}

function aiTargetKindLabel(targetKind: FileAiInsight["targetKind"]): string {
  if (targetKind === "directory") return TEXT.aiInspectTargetDirectory;
  return TEXT.aiInspectTargetFile;
}

function normalizeAiText(value: string | null | undefined): string {
  return (value ?? "")
    .trim()
    .toLowerCase()
    .replace(/[，。、“”‘’：；！？、,.!?:;"'()[\]{}\-_\s]/g, "");
}

function shouldShowSeparateReasonCard(insight: FileAiInsight): boolean {
  const normalizedSummary = normalizeAiText(insight.summary);
  const normalizedReason = normalizeAiText(insight.reason);

  if (!normalizedReason) return false;
  if (!normalizedSummary) return true;
  if (normalizedSummary === normalizedReason) return false;
  if (normalizedSummary.includes(normalizedReason)) return false;
  if (normalizedReason.includes(normalizedSummary)) return false;
  return true;
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
        <n-space vertical :size="16">
          <n-text depth="3">{{ TEXT.rootPath }}：{{ report.root }}</n-text>
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
        </n-space>
      </n-card>

      <n-card :title="TEXT.directoryOverview">
        <n-space vertical :size="12">
          <n-alert v-if="directoryOverviewError" type="error" :title="TEXT.directoryOverviewFailed">
            {{ directoryOverviewError }}
          </n-alert>
          <n-text v-else-if="directoryOverviewLoading" depth="3">
            {{ TEXT.loadingDirectoryOverview }}
          </n-text>
          <n-data-table
            v-if="directoryOverviewRows.length > 0"
            :columns="directoryColumns"
            :data="directoryOverviewRows"
            :loading="directoryOverviewLoading"
            :max-height="320"
            size="small"
            :bordered="false"
          />
          <n-empty
            v-else-if="!directoryOverviewLoading && !directoryOverviewError"
            :description="TEXT.emptyOverview"
          />
        </n-space>
      </n-card>

      <n-card :title="TEXT.scannedFiles">
        <n-space vertical :size="12">
          <n-grid :cols="2" :x-gap="12">
            <n-gi>
              <n-input
                v-model:value="fileQuery"
                clearable
                :placeholder="TEXT.fileSearchPlaceholder"
              />
            </n-gi>
            <n-gi>
              <n-select
                v-model:value="selectedCategory"
                :options="fileCategoryOptions"
                :placeholder="TEXT.fileCategoryPlaceholder"
              />
            </n-gi>
          </n-grid>

          <n-alert v-if="fileTreeError" type="error" :title="TEXT.fileTreeFailed">
            {{ fileTreeError }}
          </n-alert>
          <n-text v-else-if="fileTreeLoading" depth="3">{{ TEXT.loadingFileTree }}</n-text>
          <n-text v-if="!fileTreeError" depth="3">{{ scannedFilesHint }}</n-text>
          <n-alert v-if="fileTreeResult.truncated" type="warning">
            {{ TEXT.fileTreeTruncated }}
          </n-alert>
          <n-alert
            v-if="!shouldExpandFileTree && fileTreeResult.rows.length > 0"
            type="info"
          >
            {{ TEXT.fileTreeCollapsed }}
          </n-alert>

          <n-data-table
            v-if="fileTreeRows.length > 0"
            :columns="fileTreeColumns"
            :data="fileTreeRows"
            :loading="fileTreeLoading"
            :max-height="480"
            size="small"
            :bordered="false"
            :default-expand-all="shouldExpandFileTree"
          />
          <n-empty
            v-else-if="!fileTreeLoading && !fileTreeError"
            :description="TEXT.fileTreeEmpty"
          />
        </n-space>
      </n-card>

      <n-card :title="TEXT.scanModules">
        <n-grid :cols="3" :x-gap="12" :y-gap="12">
          <n-gi v-for="item in moduleCards" :key="item.kind">
            <n-card size="small">
              <n-statistic :label="item.label" :value="item.itemCount" />
              <n-text depth="3" style="display: block; margin-top: 8px">
                {{ item.description }}
              </n-text>
              <n-tag size="small" style="margin-top: 8px">
                {{ formatBytes(item.totalSize) }}
              </n-tag>
            </n-card>
          </n-gi>
        </n-grid>
      </n-card>

      <n-card :title="TEXT.typeDistribution">
        <v-chart :option="typeChartOption" style="height: 300px" autoresize />
      </n-card>

      <n-card v-if="report.analysis.largeFiles.length > 0" :title="TEXT.largeFiles">
        <n-space vertical :size="12">
          <n-alert type="info">{{ TEXT.sectionLimited }}</n-alert>
          <n-data-table
            :columns="fileColumns"
            :data="report.analysis.largeFiles"
            :max-height="280"
            size="small"
            :bordered="false"
          />
        </n-space>
      </n-card>

      <n-card v-if="report.analysis.temporaryFiles.length > 0" :title="TEXT.temporaryFiles">
        <n-space vertical :size="12">
          <n-alert type="info">{{ TEXT.sectionLimited }}</n-alert>
          <n-data-table
            :columns="fileColumns"
            :data="report.analysis.temporaryFiles"
            :max-height="280"
            size="small"
            :bordered="false"
          />
        </n-space>
      </n-card>

      <n-card v-if="report.analysis.archiveFiles.length > 0" :title="TEXT.archiveFiles">
        <n-space vertical :size="12">
          <n-alert type="info">{{ TEXT.sectionLimited }}</n-alert>
          <n-data-table
            :columns="fileColumns"
            :data="report.analysis.archiveFiles"
            :max-height="280"
            size="small"
            :bordered="false"
          />
        </n-space>
      </n-card>

      <n-card v-if="report.dedup.groups.length > 0" :title="TEXT.duplicateGroups">
        <n-space vertical :size="12">
          <n-alert v-if="report.dedup.truncated" type="info">
            {{ TEXT.duplicateGroupsLimited }}
          </n-alert>
          <n-card
            v-for="(group, idx) in report.dedup.groups"
            :key="group.hash"
            :title="`${TEXT.groupPrefix} ${idx + 1} ${TEXT.groupSuffix} (${formatBytes(group.totalSize)})`"
            size="small"
          >
            <n-space vertical :size="4">
              <div v-for="file in group.files" :key="file.path">
                <n-tag :type="duplicateTagType(file.path)" size="small">
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
        <n-space vertical :size="12">
          <n-tag :type="report.advisor.source.startsWith('remote') ? 'info' : 'default'" size="small">
            {{ advisorSourceLabel }}
          </n-tag>
          <n-alert v-if="report.advisor.truncated" type="info">
            {{ TEXT.suggestionsLimited }}
          </n-alert>
          <n-text style="display: block; white-space: pre-wrap">
            {{ report.advisor.summary }}
          </n-text>
        </n-space>
      </n-card>

      <n-button type="primary" @click="goToCleanup" style="width: 100%">
        {{ TEXT.goCleanup }}
      </n-button>
    </n-space>

    <n-modal v-model:show="aiInsightVisible" style="width: min(720px, calc(100vw - 32px))">
      <n-card :title="TEXT.aiInspectTitle" :bordered="false" size="small" role="dialog" aria-modal="true">
        <n-space vertical :size="12">
          <n-text depth="3">{{ TEXT.aiInspectHint }}</n-text>
          <n-text style="word-break: break-all">{{ selectedAiPath }}</n-text>

          <n-alert v-if="aiError" type="error" :title="TEXT.aiInspectFailed">
            {{ aiError }}
          </n-alert>

          <template v-else-if="aiLoading">
            <n-text>{{ TEXT.aiInspectLoading }}</n-text>
          </template>

          <template v-else-if="aiInsight">
            <n-alert
              v-if="aiInsight.usedFallback"
              type="warning"
              :title="TEXT.aiInspectFallbackTitle"
            >
              <div>{{ aiInsight.fallbackReason || "-" }}</div>
            </n-alert>

            <n-alert
              v-else-if="!aiInsight.remoteAttempted && aiInsight.source === 'local_rules'"
              type="info"
              :title="TEXT.aiInspectLocalOnlyTitle"
            >
              {{ TEXT.localRules }}
            </n-alert>

            <n-alert
              v-else-if="aiInsight.source.startsWith('remote')"
              type="success"
              :title="TEXT.aiInspectRemoteSuccess"
            >
              {{ formatSourceLabel(aiInsight.source) }}
            </n-alert>

            <n-space>
              <n-tag size="small" type="default">
                {{ aiTargetKindLabel(aiInsight.targetKind) }}
              </n-tag>
              <n-tag size="small" :type="aiInsight.source.startsWith('remote') ? 'info' : 'default'">
                {{ formatSourceLabel(aiInsight.source) }}
              </n-tag>
              <n-tag size="small" :type="actionTagType(aiInsight.suggestedAction)">
                {{ actionLabel(aiInsight.suggestedAction) }}
              </n-tag>
              <n-tag size="small" :type="riskTagType(aiInsight.risk)">
                {{ riskLabel(aiInsight.risk) }}
              </n-tag>
            </n-space>

            <n-card size="small" embedded>
              <n-text depth="3">
                {{ shouldShowSeparateReasonCard(aiInsight) ? TEXT.aiInspectSummary : TEXT.aiInspectReason }}
              </n-text>
              <n-text style="display: block; margin-top: 8px; white-space: pre-wrap">
                {{ aiInsight.summary }}
              </n-text>
            </n-card>

            <n-card v-if="shouldShowSeparateReasonCard(aiInsight)" size="small" embedded>
              <n-text depth="3">{{ TEXT.aiInspectReason }}</n-text>
              <n-text style="display: block; margin-top: 8px; white-space: pre-wrap">
                {{ aiInsight.reason }}
              </n-text>
            </n-card>

            <n-card v-if="aiInsight.usedFallback && aiInsight.fallbackReason" size="small" embedded>
              <n-text depth="3">{{ TEXT.aiInspectFallbackReason }}</n-text>
              <n-text style="display: block; margin-top: 8px; white-space: pre-wrap">
                {{ aiInsight.fallbackReason }}
              </n-text>
            </n-card>
          </template>
        </n-space>

        <template #footer>
          <n-space justify="end">
            <n-button @click="aiInsightVisible = false">{{ TEXT.close }}</n-button>
            <n-button
              type="primary"
              secondary
              :loading="aiLoading"
              :disabled="!selectedAiPath"
              @click="inspectFileWithAi(selectedAiPath)"
            >
              {{ TEXT.retry }}
            </n-button>
          </n-space>
        </template>
      </n-card>
    </n-modal>
  </div>
</template>
