<script setup lang="ts">
import { computed, h, ref } from "vue";
import { useRouter } from "vue-router";
import {
  NAlert,
  NButton,
  NCard,
  NDataTable,
  NEmpty,
  NGi,
  NModal,
  NGrid,
  NInput,
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
  FileAiInsight,
  FileRecord,
  FileSuggestion,
  ScanModuleKind,
  ScanModuleSummary,
  SuggestedAction,
} from "@/types";

interface DirectoryOverviewRow {
  key: string;
  name: string;
  path: string;
  kind: "directory" | "file";
  fileCount: number;
  totalSize: number;
  preview: string;
}

interface DirectoryOverviewBucket {
  key: string;
  name: string;
  path: string;
  kind: "directory" | "file";
  fileCount: number;
  totalSize: number;
  preview: Set<string>;
}

interface FileTreeRow {
  key: string;
  name: string;
  path: string;
  kind: "directory" | "file";
  size: number;
  extension: string;
  fileCount: number;
  children?: FileTreeRow[];
}

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
  scannedFilesHintMiddle: " 个文件，按目录分层展示 ",
  scannedFilesHintSuffix: " 个节点。",
  fileSearchPlaceholder: "按文件名、路径或扩展名筛选，例如 pdf、src、README",
  fileCategoryPlaceholder: "按文件类型筛选",
  fileCategory: "文件类型筛选",
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
  aiInspectHint: "支持对单个文件或目录调用 AI。目录分析只会上传文件名、扩展名、大小、数量等摘要信息，不会读取整个目录下所有文件内容，用来减少 token 消耗。",
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
};

use([PieChart, TitleComponent, TooltipComponent, LegendComponent, CanvasRenderer]);

const router = useRouter();
const store = useAppStore();
const { loading: aiLoading, error: aiError, explainFile } = useAiFile();
const report = computed(() => store.report);
const fileQuery = ref("");
const selectedCategory = ref<FileCategory>("all");
const aiInsightVisible = ref(false);
const aiInsight = ref<FileAiInsight | null>(null);
const selectedAiPath = ref("");

const IMAGE_EXTENSIONS = new Set([
  "png",
  "jpg",
  "jpeg",
  "gif",
  "bmp",
  "webp",
  "svg",
  "ico",
  "tif",
  "tiff",
  "heic",
  "heif",
  "raw",
  "psd",
  "avif",
]);

const VIDEO_EXTENSIONS = new Set([
  "mp4",
  "mkv",
  "avi",
  "mov",
  "wmv",
  "flv",
  "webm",
  "m4v",
  "mpeg",
  "mpg",
  "ts",
  "3gp",
  "rmvb",
]);

const AUDIO_EXTENSIONS = new Set([
  "mp3",
  "wav",
  "flac",
  "aac",
  "m4a",
  "ogg",
  "wma",
  "opus",
  "ape",
  "amr",
  "mid",
  "midi",
]);

const ARCHIVE_EXTENSIONS = new Set([
  "zip",
  "zipx",
  "7z",
  "rar",
  "tar",
  "gz",
  "tgz",
  "bz2",
  "xz",
  "cab",
  "iso",
  "img",
  "dmg",
  "jar",
]);

const EXECUTABLE_EXTENSIONS = new Set([
  "exe",
  "com",
  "msi",
  "msix",
  "msixbundle",
  "appx",
  "appxbundle",
  "bat",
  "cmd",
  "ps1",
  "vbs",
  "js",
  "jar",
  "scr",
]);

const DOCUMENT_EXTENSIONS = new Set([
  "pdf",
  "doc",
  "docx",
  "ppt",
  "pptx",
  "xls",
  "xlsx",
  "csv",
  "txt",
  "md",
  "rtf",
  "wps",
  "odt",
  "ods",
  "odp",
]);

const CODE_EXTENSIONS = new Set([
  "rs",
  "toml",
  "json",
  "yaml",
  "yml",
  "xml",
  "ini",
  "cfg",
  "conf",
  "env",
  "ts",
  "tsx",
  "js",
  "jsx",
  "vue",
  "py",
  "java",
  "kt",
  "go",
  "c",
  "cc",
  "cpp",
  "h",
  "hpp",
  "cs",
  "php",
  "rb",
  "swift",
  "scala",
  "sql",
  "html",
  "css",
  "scss",
  "less",
  "sh",
  "bash",
  "zsh",
  "ps1",
  "lock",
]);

const suggestionByPath = computed(() => {
  const map = new Map<string, FileSuggestion>();
  for (const item of report.value?.advisor.suggestions ?? []) {
    map.set(item.path, item);
  }
  return map;
});

const advisorSourceLabel = computed(() => {
  return formatSourceLabel(report.value?.advisor.source);
});

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
        data: breakdown.map((t) => ({
          name: t.extension || TEXT.noExt,
          value: t.totalSize,
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
    sorter: (a, b) => a.size - b.size,
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
    sorter: (a, b) => a.size - b.size,
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

const directoryOverviewRows = computed<DirectoryOverviewRow[]>(() => {
  if (!report.value) return [];

  const buckets = new Map<string, DirectoryOverviewBucket>();

  for (const file of report.value.scannedFiles) {
    const parts = relativeParts(file.path, report.value.root);
    if (parts.length === 0) continue;

    if (parts.length === 1) {
      buckets.set(`file:${parts[0]}`, {
        key: `file:${parts[0]}`,
        name: parts[0],
        path: file.path,
        kind: "file",
        fileCount: 1,
        totalSize: file.size,
        preview: new Set(),
      });
      continue;
    }

    const key = `dir:${parts[0]}`;
    const bucket =
      buckets.get(key) ??
      {
        key,
        name: parts[0],
        path: report.value.root ? `${report.value.root}/${parts[0]}` : parts[0],
        kind: "directory" as const,
        fileCount: 0,
        totalSize: 0,
        preview: new Set<string>(),
      };

    bucket.fileCount += 1;
    bucket.totalSize += file.size;
    if (parts[1]) {
      bucket.preview.add(parts[1]);
    }
    buckets.set(key, bucket);
  }

  for (const dirPath of report.value.analysis.emptyDirs) {
    const parts = relativeParts(dirPath, report.value.root);
    if (parts.length === 0) continue;

    const key = `dir:${parts[0]}`;
    const bucket =
      buckets.get(key) ??
      {
        key,
        name: parts[0],
        path: report.value.root ? `${report.value.root}/${parts[0]}` : parts[0],
        kind: "directory" as const,
        fileCount: 0,
        totalSize: 0,
        preview: new Set<string>(),
      };

    if (parts[1]) {
      bucket.preview.add(parts[1]);
    }
    buckets.set(key, bucket);
  }

  return Array.from(buckets.values())
    .map((item) => ({
      key: item.key,
      name: item.name,
      path: item.path,
      kind: item.kind,
      fileCount: item.fileCount,
      totalSize: item.totalSize,
      preview:
        item.kind === "file"
          ? "-"
          : item.preview.size > 0
            ? `${Array.from(item.preview).slice(0, 4).join("、")}${item.preview.size > 4 ? " 等" : ""}`
            : "空目录",
    }))
    .sort((left, right) => {
      if (left.kind !== right.kind) {
        return left.kind === "directory" ? -1 : 1;
      }
      return left.name.localeCompare(right.name, "zh-CN");
    });
});

const matchedScannedFiles = computed(() => {
  const files = report.value?.scannedFiles ?? [];
  const query = fileQuery.value.trim().toLowerCase();
  return files
    .filter((file) => {
      const ext = (file.extension ?? "").toLowerCase();
      const fullPath = file.path.toLowerCase();
      const queryMatched = !query || fullPath.includes(query) || ext.includes(query);
      const categoryMatched = matchesFileCategory(file, selectedCategory.value);
      return queryMatched && categoryMatched;
    })
    .sort((a, b) => a.path.localeCompare(b.path, "zh-CN"));
});

const fileTreeRows = computed<FileTreeRow[]>(() => {
  if (!report.value) return [];

  const root = report.value.root;
  const nodeMap = new Map<
    string,
    {
      row: FileTreeRow;
      children: Map<string, string>;
    }
  >();

  const roots: string[] = [];

  const ensureDirectory = (parts: string[], depth: number) => {
    const key = `dir:${parts.slice(0, depth + 1).join("/")}`;
    if (!nodeMap.has(key)) {
      const relativePath = parts.slice(0, depth + 1).join("/");
      nodeMap.set(key, {
        row: {
          key,
          name: parts[depth],
          path: relativePath,
          kind: "directory",
          size: 0,
          extension: "",
          fileCount: 0,
          children: [],
        },
        children: new Map(),
      });

      if (depth === 0) {
        roots.push(key);
      } else {
        const parentKey = `dir:${parts.slice(0, depth).join("/")}`;
        const parent = nodeMap.get(parentKey);
        parent?.children.set(key, key);
      }
    }
    return nodeMap.get(key)!;
  };

  for (const file of matchedScannedFiles.value) {
    const parts = relativeParts(file.path, root);
    if (parts.length === 0) continue;

    if (parts.length === 1) {
      const key = `file:${parts[0]}`;
      nodeMap.set(key, {
        row: {
          key,
          name: parts[0],
          path: parts[0],
          kind: "file",
          size: file.size,
          extension: file.extension ?? "",
          fileCount: 1,
        },
        children: new Map(),
      });
      roots.push(key);
      continue;
    }

    for (let depth = 0; depth < parts.length - 1; depth += 1) {
      const node = ensureDirectory(parts, depth);
      node.row.size += file.size;
      node.row.fileCount += 1;
    }

    const fileKey = `file:${parts.join("/")}`;
    const fileRow: FileTreeRow = {
      key: fileKey,
      name: parts[parts.length - 1],
      path: parts.join("/"),
      kind: "file",
      size: file.size,
      extension: file.extension ?? "",
      fileCount: 1,
    };
    nodeMap.set(fileKey, { row: fileRow, children: new Map() });

    const parentKey = `dir:${parts.slice(0, parts.length - 1).join("/")}`;
    nodeMap.get(parentKey)?.children.set(fileKey, fileKey);
  }

  const buildRows = (keys: string[]): FileTreeRow[] =>
    keys
      .map((key) => nodeMap.get(key))
      .filter((value): value is NonNullable<typeof value> => Boolean(value))
      .map((entry) => {
        const childKeys = Array.from(entry.children.keys()).sort((left, right) => {
          const leftRow = nodeMap.get(left)?.row;
          const rightRow = nodeMap.get(right)?.row;
          if (!leftRow || !rightRow) return 0;
          if (leftRow.kind !== rightRow.kind) {
            return leftRow.kind === "directory" ? -1 : 1;
          }
          if (leftRow.size !== rightRow.size) {
            return rightRow.size - leftRow.size;
          }
          return leftRow.name.localeCompare(rightRow.name, "zh-CN");
        });

        return {
          ...entry.row,
          children: childKeys.length > 0 ? buildRows(childKeys) : undefined,
        };
      });

  const rootRows = buildRows(
    Array.from(new Set(roots)).sort((left, right) => {
      const leftRow = nodeMap.get(left)?.row;
      const rightRow = nodeMap.get(right)?.row;
      if (!leftRow || !rightRow) return 0;
      if (leftRow.kind !== rightRow.kind) {
        return leftRow.kind === "directory" ? -1 : 1;
      }
      if (leftRow.size !== rightRow.size) {
        return rightRow.size - leftRow.size;
      }
      return leftRow.name.localeCompare(rightRow.name, "zh-CN");
    })
  );

  return rootRows;
});

const scannedFilesHint = computed(() => {
  return `${TEXT.scannedFilesHintPrefix}${matchedScannedFiles.value.length}${TEXT.scannedFilesHintMiddle}${countTreeNodes(fileTreeRows.value)}${TEXT.scannedFilesHintSuffix}`;
});

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

function actionTagType(action: SuggestedAction): "default" | "success" | "warning" | "error" | "info" {
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

function normalizePath(value: string): string {
  return value.replace(/\\/g, "/").replace(/\/+/g, "/").replace(/\/$/, "");
}

function relativeParts(fullPath: string, rootPath: string): string[] {
  const normalizedRoot = normalizePath(rootPath);
  const normalizedPath = normalizePath(fullPath);

  if (normalizedPath === normalizedRoot) {
    return [];
  }

  if (normalizedPath.startsWith(`${normalizedRoot}/`)) {
    return normalizedPath.slice(normalizedRoot.length + 1).split("/").filter(Boolean);
  }

  return normalizedPath.split("/").filter(Boolean);
}

function countTreeNodes(rows: FileTreeRow[]): number {
  return rows.reduce((total, row) => {
    return total + 1 + countTreeNodes(row.children ?? []);
  }, 0);
}

function matchesFileCategory(file: FileRecord, category: FileCategory): boolean {
  if (category === "all") return true;

  const ext = (file.extension ?? "").toLowerCase();
  if (!ext) {
    return category === "other";
  }

  if (category === "image") return IMAGE_EXTENSIONS.has(ext);
  if (category === "video") return VIDEO_EXTENSIONS.has(ext);
  if (category === "audio") return AUDIO_EXTENSIONS.has(ext);
  if (category === "archive") return ARCHIVE_EXTENSIONS.has(ext);
  if (category === "executable") return EXECUTABLE_EXTENSIONS.has(ext);
  if (category === "document") return DOCUMENT_EXTENSIONS.has(ext);
  if (category === "code") return CODE_EXTENSIONS.has(ext);

  return !(
    IMAGE_EXTENSIONS.has(ext) ||
    VIDEO_EXTENSIONS.has(ext) ||
    AUDIO_EXTENSIONS.has(ext) ||
    ARCHIVE_EXTENSIONS.has(ext) ||
    EXECUTABLE_EXTENSIONS.has(ext) ||
    DOCUMENT_EXTENSIONS.has(ext) ||
    CODE_EXTENSIONS.has(ext)
  );
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
              <n-statistic :label="TEXT.duplicateGroups" :value="report.dedup.groups.length" />
            </n-gi>
            <n-gi>
              <n-statistic :label="TEXT.suggestionCount" :value="report.advisor.suggestions.length" />
            </n-gi>
          </n-grid>
        </n-space>
      </n-card>

      <n-card :title="TEXT.directoryOverview">
        <n-data-table
          v-if="directoryOverviewRows.length > 0"
          :columns="directoryColumns"
          :data="directoryOverviewRows"
          :max-height="320"
          size="small"
          :bordered="false"
        />
        <n-empty v-else :description="TEXT.emptyOverview" />
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
          <n-text depth="3">{{ scannedFilesHint }}</n-text>
          <n-data-table
            :columns="fileTreeColumns"
            :data="fileTreeRows"
            :max-height="480"
            size="small"
            :bordered="false"
            default-expand-all
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
        <n-data-table
          :columns="fileColumns"
          :data="report.analysis.largeFiles.slice(0, 50)"
          :max-height="280"
          size="small"
          :bordered="false"
        />
      </n-card>

      <n-card v-if="report.analysis.temporaryFiles.length > 0" :title="TEXT.temporaryFiles">
        <n-data-table
          :columns="fileColumns"
          :data="report.analysis.temporaryFiles.slice(0, 50)"
          :max-height="280"
          size="small"
          :bordered="false"
        />
      </n-card>

      <n-card v-if="report.analysis.archiveFiles.length > 0" :title="TEXT.archiveFiles">
        <n-data-table
          :columns="fileColumns"
          :data="report.analysis.archiveFiles.slice(0, 50)"
          :max-height="280"
          size="small"
          :bordered="false"
        />
      </n-card>

      <n-card v-if="report.dedup.groups.length > 0" :title="TEXT.duplicateGroups">
        <n-space vertical :size="12">
          <n-card
            v-for="(group, idx) in report.dedup.groups.slice(0, 10)"
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
        <n-tag :type="report.advisor.source.startsWith('remote') ? 'info' : 'default'" size="small">
          {{ advisorSourceLabel }}
        </n-tag>
        <n-text style="display: block; margin-top: 12px; white-space: pre-wrap">
          {{ report.advisor.summary }}
        </n-text>
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
              <div>{{ aiInsight.fallbackReason || '-' }}</div>
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
