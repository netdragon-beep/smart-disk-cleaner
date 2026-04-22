<script setup lang="ts">
import { computed, h, onBeforeUnmount, onMounted, ref } from "vue";
import {
  NAlert,
  NButton,
  NCard,
  NDataTable,
  NEmpty,
  NInput,
  NModal,
  NSelect,
  NSpace,
  NTag,
  NText,
  useMessage,
  type DataTableColumns,
} from "naive-ui";
import VChart from "vue-echarts";
import { use } from "echarts/core";
import { LineChart } from "echarts/charts";
import {
  GridComponent,
  MarkLineComponent,
  TooltipComponent,
} from "echarts/components";
import { CanvasRenderer } from "echarts/renderers";
import { useProcesses } from "@/composables/useProcesses";
import { useAppStore } from "@/stores/app";
import type {
  ProcessAiFollowUpAnswer,
  ProcessAiFollowUpTurn,
  ProcessAiInsight,
  ProcessMonitorSnapshot,
  ProcessRecord,
  ProcessSuggestedAction,
  RiskLevel,
} from "@/types";

use([LineChart, GridComponent, TooltipComponent, MarkLineComponent, CanvasRenderer]);

type ProcessSortMode = "score" | "cpu" | "memory" | "disk" | "runtime" | "name";
type SortDirection = "desc" | "asc";

const TEXT = {
  title: "卡顿监控",
  subtitle: "持续采样 CPU、内存和磁盘活动，帮助回放卡顿发生时的进程状态。",
  startMonitor: "开始监控",
  stopMonitor: "停止监控",
  clearHistory: "清空记录",
  sampling: "监控中",
  idle: "未监控",
  refreshNow: "立即采样",
  anomalyTitle: "异常时刻",
  anomalyEmpty: "目前还没有明显异常。开始监控后复现一次卡顿，更容易定位问题。",
  snapshotTitle: "时刻进程快照",
  snapshotEmpty: "暂无进程快照，请先开始监控或手动采样。",
  selectedTime: "当前时刻",
  queryPlaceholder: "按进程名、路径、命令行或 PID 搜索",
  sortByScore: "按综合压力",
  sortByCpu: "按 CPU",
  sortByMemory: "按内存",
  sortByDisk: "按磁盘",
  sortByRuntime: "按运行时长",
  sortByName: "按进程名",
  sortOrderDesc: "从大到小",
  sortOrderAsc: "从小到大",
  aiInspect: "AI 解读",
  terminate: "结束进程",
  protected: "受保护",
  name: "进程名",
  pid: "PID",
  category: "类别",
  cpu: "CPU",
  memory: "内存",
  disk: "磁盘活动",
  score: "资源压力",
  runtime: "运行时长",
  actions: "操作",
  aiDialogTitle: "AI 进程解读",
  aiFailed: "AI 解读失败",
  remoteOk: "已调用远程 AI 模型",
  fallbackTitle: "远程 AI 调用失败，已回退到本地规则",
  localOnlyTitle: "当前显示的是本地规则分析结果",
  summary: "结论",
  reason: "原因说明",
  followUpTitle: "进一步提问",
  followUpSubtitle: "你可以针对这个进程继续追问用途、风险、影响或处理建议。",
  followUpPlaceholder: "例如：现在结束它会有什么影响？",
  askFollowUp: "发送提问",
  followUpEmpty: "还没有追问记录。你可以先点一个推荐问题，或者自己输入问题。",
  followUpFailed: "进一步解读失败",
  close: "关闭",
  retry: "重新解读",
  terminateDialogTitle: "确认结束进程",
  terminateHint: "这个操作会立刻结束目标进程，请先确认没有未保存的工作。",
  terminateFailed: "结束进程失败",
  confirmTerminate: "确认结束",
  protectedHint: "系统关键进程和安全进程不会提供结束操作。",
  targetProcess: "目标进程",
  executablePath: "可执行文件",
  commandLine: "命令行",
  pendingInsight: "后台正在解读这个进程。你可以先继续查看其他内容，稍后再回来查看结果。",
  monitoringFailed: "监控采样失败",
  chartHint: "点击任意图表上的采样点，可以回放那个时刻的进程快照。",
  chartCpuTitle: "CPU 变化",
  chartCpuDesc: "适合观察瞬时占用飙升和卡顿尖峰。",
  chartMemoryTitle: "内存占用",
  chartMemoryDesc: "适合观察持续上涨、泄漏或占满的情况。",
  chartDiskTitle: "磁盘吞吐",
  chartDiskDesc: "适合观察大量读写、索引、同步和解压场景。",
};

const MONITOR_INTERVAL_MS = 1000;
const MAX_SAMPLES = 300;
const TOP_PROCESS_LIMIT = 12;
const DISK_ALERT_BYTES = 20 * 1024 * 1024;

const store = useAppStore();
const message = useMessage();
const {
  error,
  followUpError,
  followUpLoading,
  terminating,
  terminateError,
  askProcessFollowUp,
  requestProcessInsight,
  loadMonitorSnapshot,
  terminateProcess,
} = useProcesses();

const monitoring = ref(false);
const sampleLoadingCount = ref(0);
const manualSampling = ref(false);
const lastSampleAt = ref<string | null>(null);
const samples = ref<ProcessMonitorSnapshot[]>([]);
const selectedSampleIndex = ref<number | null>(null);
const processQuery = ref("");
const sortBy = ref<ProcessSortMode>("score");
const sortDirection = ref<SortDirection>("desc");

const aiVisible = ref(false);
const selectedProcess = ref<ProcessRecord | null>(null);
const processInsightCache = ref<Record<number, ProcessAiInsight>>({});
const processInsightPending = ref<Record<number, boolean>>({});
const processInsightErrors = ref<Record<number, string>>({});
const processFollowUpInput = ref("");
const processFollowUpCache = ref<Record<number, ProcessAiFollowUpAnswer[]>>({});

const terminateVisible = ref(false);
const terminateTarget = ref<ProcessRecord | null>(null);

let monitorTimer: ReturnType<typeof setInterval> | null = null;

const sampleLoading = computed(() => sampleLoadingCount.value > 0);

const sortOptions = [
  { label: TEXT.sortByScore, value: "score" },
  { label: TEXT.sortByCpu, value: "cpu" },
  { label: TEXT.sortByMemory, value: "memory" },
  { label: TEXT.sortByDisk, value: "disk" },
  { label: TEXT.sortByRuntime, value: "runtime" },
  { label: TEXT.sortByName, value: "name" },
];

const latestSample = computed(() =>
  samples.value.length > 0 ? samples.value[samples.value.length - 1] : null
);

const selectedSample = computed(() => {
  if (samples.value.length === 0) return null;
  if (selectedSampleIndex.value === null) return samples.value[samples.value.length - 1];
  return samples.value[selectedSampleIndex.value] ?? samples.value[samples.value.length - 1];
});

const selectedSampleTime = computed(() =>
  selectedSample.value ? formatTime(selectedSample.value.collectedAt) : "-"
);
const latestSampleTimeLabel = computed(() =>
  lastSampleAt.value ? formatTime(lastSampleAt.value) : "-"
);
const latestCpuLabel = computed(() =>
  latestSample.value ? `${latestSample.value.systemCpuUsage.toFixed(1)}%` : "-"
);
const latestMemoryLabel = computed(() =>
  latestSample.value ? `${memoryUsagePercent(latestSample.value).toFixed(1)}%` : "-"
);
const latestDiskLabel = computed(() =>
  latestSample.value ? `${formatBytes(latestSample.value.diskBytesPerSec)}/s` : "-"
);
const peakCpuLabel = computed(() => {
  if (samples.value.length === 0) return "-";
  return `${Math.max(...samples.value.map((item) => item.systemCpuUsage)).toFixed(1)}%`;
});
const peakMemoryLabel = computed(() => {
  if (samples.value.length === 0) return "-";
  return `${Math.max(...samples.value.map((item) => memoryUsagePercent(item))).toFixed(1)}%`;
});
const peakDiskLabel = computed(() => {
  if (samples.value.length === 0) return "-";
  return `${formatBytes(Math.max(...samples.value.map((item) => item.diskBytesPerSec)))}/s`;
});

const filteredProcesses = computed(() => {
  const snapshot = selectedSample.value;
  if (!snapshot) return [];
  const keyword = processQuery.value.trim().toLowerCase();
  return snapshot.topProcesses.filter((item) => {
    if (!keyword) return true;
    const haystack = [item.name, item.exePath ?? "", item.command.join(" "), String(item.pid)]
      .join(" ")
      .toLowerCase();
    return haystack.includes(keyword);
  });
});

const sortedProcesses = computed(() => {
  const rows = [...filteredProcesses.value];
  const multiplier = sortDirection.value === "desc" ? -1 : 1;
  rows.sort((left, right) => {
    const result = compareProcesses(left, right, sortBy.value);
    if (result !== 0) return result * multiplier;
    return left.name.localeCompare(right.name);
  });
  return rows;
});

const xAxisLabels = computed(() => samples.value.map((item) => formatTime(item.collectedAt)));
const selectedXAxisLabel = computed(() => {
  if (selectedSampleIndex.value === null) return undefined;
  return xAxisLabels.value[selectedSampleIndex.value];
});

const cpuChartOption = computed(() =>
  buildChartOption({
    labels: xAxisLabels.value,
    data: samples.value.map((item) => Number(item.systemCpuUsage.toFixed(1))),
    selectedLabel: selectedXAxisLabel.value,
    color: "#2f6df6",
    axisName: "%",
    max: 100,
    tooltip: (value) => `${Number(value).toFixed(1)}%`,
  })
);

const memoryChartOption = computed(() =>
  buildChartOption({
    labels: xAxisLabels.value,
    data: samples.value.map((item) => Number(memoryUsagePercent(item).toFixed(1))),
    selectedLabel: selectedXAxisLabel.value,
    color: "#1ea97c",
    axisName: "%",
    max: 100,
    tooltip: (value) => `${Number(value).toFixed(1)}%`,
  })
);

const diskChartOption = computed(() =>
  buildChartOption({
    labels: xAxisLabels.value,
    data: samples.value.map((item) => item.diskBytesPerSec),
    selectedLabel: selectedXAxisLabel.value,
    color: "#ef8a22",
    axisName: "B/s",
    tooltip: (value) => `${formatBytes(Number(value))}/s`,
    axisLabel: (value) => formatCompactBytes(Number(value)),
  })
);

const anomalySamples = computed(() =>
  samples.value
    .map((sample, index) => {
      const cpuScore = sample.systemCpuUsage / 85;
      const memoryScore = memoryUsagePercent(sample) / 85;
      const diskScore = sample.diskBytesPerSec / DISK_ALERT_BYTES;
      return { index, sample, severity: Math.max(cpuScore, memoryScore, diskScore) };
    })
    .filter((item) => item.severity >= 1)
    .sort((left, right) => right.severity - left.severity)
    .slice(0, 12)
);

const selectedProcessInsight = computed(() =>
  selectedProcess.value ? processInsightCache.value[selectedProcess.value.pid] ?? null : null
);
const selectedProcessInsightPending = computed(() =>
  selectedProcess.value ? Boolean(processInsightPending.value[selectedProcess.value.pid]) : false
);
const selectedProcessInsightError = computed(() =>
  selectedProcess.value ? processInsightErrors.value[selectedProcess.value.pid] ?? null : null
);
const selectedProcessFollowUps = computed(() =>
  selectedProcess.value ? processFollowUpCache.value[selectedProcess.value.pid] ?? [] : []
);

const suggestedFollowUpQuestions = computed(() => {
  if (!selectedProcess.value) return [];
  return [
    "这个进程主要是做什么的？",
    "现在结束它会有什么影响？",
    "它为什么会占用这么多资源？",
    "如果先不处理，可能会有什么后果？",
  ];
});

const columns = computed<DataTableColumns<ProcessRecord>>(() => [
  {
    title: TEXT.name,
    key: "name",
    minWidth: 240,
    render: (row) =>
      h("div", { style: "display: flex; flex-direction: column; gap: 4px;" }, [
        h(
          "div",
          { style: "display: flex; align-items: center; gap: 8px; flex-wrap: wrap;" },
          [
            h("span", { style: "font-weight: 600;" }, row.name),
            row.isCritical
              ? h(NTag, { size: "small", type: "error" }, () => TEXT.protected)
              : null,
          ]
        ),
        h(NText, { depth: 3, style: "font-size: 12px;" }, () => row.exePath || "-"),
      ]),
  },
  { title: TEXT.pid, key: "pid", width: 88 },
  {
    title: TEXT.category,
    key: "category",
    width: 120,
    render: (row) =>
      h(NTag, { size: "small", type: categoryTagType(row.category) }, () =>
        categoryLabel(row.category)
      ),
  },
  { title: TEXT.cpu, key: "cpuUsage", width: 92, render: (row) => `${row.cpuUsage.toFixed(1)}%` },
  { title: TEXT.memory, key: "memoryBytes", width: 112, render: (row) => formatBytes(row.memoryBytes) },
  {
    title: TEXT.disk,
    key: "disk",
    width: 122,
    render: (row) => `${formatBytes(row.diskReadBytes + row.diskWrittenBytes)}/s`,
  },
  { title: TEXT.score, key: "resourceScore", width: 98, render: (row) => row.resourceScore.toFixed(1) },
  { title: TEXT.runtime, key: "runTimeSeconds", width: 122, render: (row) => formatRuntime(row.runTimeSeconds) },
  {
    title: TEXT.actions,
    key: "actions",
    width: 192,
    fixed: "right",
    render: (row) =>
      h("div", { style: "display: flex; justify-content: flex-end; gap: 8px;" }, [
        h(
          NButton,
          {
            size: "tiny",
            secondary: true,
            type: "primary",
            loading: Boolean(processInsightPending.value[row.pid]),
            onClick: () => void handleProcessAiAction(row),
          },
          () => processActionButtonText(row.pid)
        ),
        h(
          NButton,
          {
            size: "tiny",
            secondary: true,
            type: isProtectedProcess(row) ? "default" : "error",
            disabled: isProtectedProcess(row),
            onClick: () => openTerminateDialog(row),
          },
          () => TEXT.terminate
        ),
      ]),
  },
]);

onMounted(async () => {
  await sampleOnce();
});

onBeforeUnmount(() => {
  stopMonitoring();
});

async function sampleOnce(options: { manual?: boolean } = {}) {
  if (!options.manual && sampleLoading.value) {
    return;
  }

  if (options.manual && manualSampling.value) {
    message.info("立即采样已经在执行中，请稍候。");
    return;
  }

  sampleLoadingCount.value += 1;
  if (options.manual) {
    manualSampling.value = true;
  }
  try {
    const snapshot = await loadMonitorSnapshot(TOP_PROCESS_LIMIT);
    if (!snapshot) {
      if (options.manual) {
        message.error("立即采样失败，请稍后重试。");
      }
      return;
    }
    const previousLength = samples.value.length;
    const previousSelectedSnapshot =
      selectedSampleIndex.value !== null ? samples.value[selectedSampleIndex.value] ?? null : null;
    const wasViewingLatest =
      selectedSampleIndex.value === null ||
      selectedSampleIndex.value >= previousLength - 1;
    const next = [...samples.value, snapshot].sort(
      (left, right) =>
        new Date(left.collectedAt).getTime() - new Date(right.collectedAt).getTime()
    );
    if (next.length > MAX_SAMPLES) {
      next.splice(0, next.length - MAX_SAMPLES);
    }
    samples.value = next;
    lastSampleAt.value = snapshot.collectedAt;
    if (wasViewingLatest) {
      selectedSampleIndex.value = next.length - 1;
    } else if (selectedSampleIndex.value !== null) {
      if (previousSelectedSnapshot) {
        const nextIndex = next.findIndex(
          (item) => item.collectedAt === previousSelectedSnapshot.collectedAt
        );
        selectedSampleIndex.value = nextIndex >= 0 ? nextIndex : Math.max(0, next.length - 1);
      }
    }

    if (options.manual) {
      message.success(`已完成立即采样：${formatTime(snapshot.collectedAt)}`);
    }
  } finally {
    sampleLoadingCount.value = Math.max(0, sampleLoadingCount.value - 1);
    if (options.manual) {
      manualSampling.value = false;
    }
  }
}

async function startMonitoring() {
  if (monitoring.value) return;
  monitoring.value = true;
  await sampleOnce();
  monitorTimer = setInterval(() => {
    void sampleOnce();
  }, MONITOR_INTERVAL_MS);
}

function stopMonitoring() {
  monitoring.value = false;
  if (!monitorTimer) return;
  clearInterval(monitorTimer);
  monitorTimer = null;
}

function clearHistory() {
  samples.value = [];
  selectedSampleIndex.value = null;
  lastSampleAt.value = null;
}

async function handleManualSample() {
  await sampleOnce({ manual: true });
}

function selectSample(index: number) {
  selectedSampleIndex.value = index;
}

function handleChartClick(params: { dataIndex?: number }) {
  if (typeof params.dataIndex === "number") selectSample(params.dataIndex);
}

function processActionButtonText(pid: number) {
  if (processInsightPending.value[pid]) return "解读中";
  if (processInsightCache.value[pid]) return "查看解读";
  if (processInsightErrors.value[pid]) return "重试解读";
  return TEXT.aiInspect;
}

async function queueProcessInsight(process: ProcessRecord, force = false) {
  if (processInsightPending.value[process.pid]) return;
  if (!force && processInsightCache.value[process.pid]) return;
  processInsightPending.value = { ...processInsightPending.value, [process.pid]: true };
  const nextErrors = { ...processInsightErrors.value };
  delete nextErrors[process.pid];
  processInsightErrors.value = nextErrors;
  try {
    const result = await requestProcessInsight(process.pid, store.config);
    processInsightCache.value = { ...processInsightCache.value, [process.pid]: result };
  } catch (reason: any) {
    processInsightErrors.value = {
      ...processInsightErrors.value,
      [process.pid]: typeof reason === "string" ? reason : reason?.message || String(reason),
    };
  } finally {
    processInsightPending.value = { ...processInsightPending.value, [process.pid]: false };
  }
}

async function handleProcessAiAction(process: ProcessRecord) {
  selectedProcess.value = process;
  processFollowUpInput.value = "";
  if (processInsightCache.value[process.pid] || processInsightErrors.value[process.pid]) {
    aiVisible.value = true;
    return;
  }
  if (processInsightPending.value[process.pid]) {
    message.info("这个进程正在后台解读，稍后再点“查看解读”即可。");
    return;
  }
  void queueProcessInsight(process);
  message.info("已加入后台 AI 解读，你可以继续浏览其他进程。");
}

function retrySelectedProcessInsight() {
  if (!selectedProcess.value) return;
  void queueProcessInsight(selectedProcess.value, true);
  message.info("已重新加入后台解读队列。");
}

function fillFollowUpQuestion(question: string) {
  processFollowUpInput.value = question;
}

async function submitFollowUp(questionOverride?: string) {
  if (!selectedProcess.value) return;
  if (!selectedProcessInsight.value || selectedProcessInsightPending.value) {
    message.warning("请先等待当前进程的基础解读完成，再继续追问。");
    return;
  }

  const question = (questionOverride ?? processFollowUpInput.value).trim();
  if (!question) {
    message.warning("请输入你想进一步了解的问题。");
    return;
  }

  const history: ProcessAiFollowUpTurn[] = selectedProcessFollowUps.value.map((item) => ({
    question: item.question,
    answer: item.answer,
  }));

  const answer = await askProcessFollowUp(
    selectedProcess.value.pid,
    question,
    history,
    store.config
  );
  if (!answer) {
    message.error("进一步解读失败，请稍后重试。");
    return;
  }

  const current = processFollowUpCache.value[selectedProcess.value.pid] ?? [];
  processFollowUpCache.value = {
    ...processFollowUpCache.value,
    [selectedProcess.value.pid]: [...current, answer],
  };
  processFollowUpInput.value = "";
}

function openTerminateDialog(process: ProcessRecord) {
  terminateTarget.value = process;
  terminateVisible.value = true;
}

async function confirmTerminate() {
  if (!terminateTarget.value) return;
  const result = await terminateProcess(terminateTarget.value.pid);
  if (!result) return;
  message.success(result);
  terminateVisible.value = false;
  terminateTarget.value = null;
  await sampleOnce();
}

function compareProcesses(left: ProcessRecord, right: ProcessRecord, mode: ProcessSortMode) {
  if (mode === "cpu") return left.cpuUsage - right.cpuUsage;
  if (mode === "memory") return left.memoryBytes - right.memoryBytes;
  if (mode === "disk") {
    return left.diskReadBytes + left.diskWrittenBytes - (right.diskReadBytes + right.diskWrittenBytes);
  }
  if (mode === "runtime") return left.runTimeSeconds - right.runTimeSeconds;
  if (mode === "name") return left.name.localeCompare(right.name);
  return left.resourceScore - right.resourceScore;
}

function toggleSortDirection() {
  sortDirection.value = sortDirection.value === "desc" ? "asc" : "desc";
}

function memoryUsagePercent(sample: ProcessMonitorSnapshot) {
  if (!sample.memoryTotalBytes) return 0;
  return (sample.memoryUsedBytes / sample.memoryTotalBytes) * 100;
}

function anomalyLabel(sample: ProcessMonitorSnapshot) {
  const reasons: string[] = [];
  if (sample.systemCpuUsage >= 85) reasons.push(`CPU ${sample.systemCpuUsage.toFixed(1)}%`);
  const memoryPercent = memoryUsagePercent(sample);
  if (memoryPercent >= 85) reasons.push(`内存 ${memoryPercent.toFixed(1)}%`);
  if (sample.diskBytesPerSec >= DISK_ALERT_BYTES) reasons.push(`磁盘 ${formatBytes(sample.diskBytesPerSec)}/s`);
  return reasons.join(" / ") || "资源异常";
}

function anomalySeverityType(severity: number): "warning" | "error" {
  return severity >= 1.35 ? "error" : "warning";
}

function isProtectedProcess(process: ProcessRecord) {
  return process.isCritical || process.category === "system_critical" || process.category === "security";
}

function categoryLabel(category: string) {
  if (category === "system_critical") return "系统关键";
  if (category === "system_service") return "系统服务";
  if (category === "windows_component") return "Windows 组件";
  if (category === "security") return "安全防护";
  if (category === "browser") return "浏览器";
  if (category === "development") return "开发工具";
  if (category === "virtualization") return "容器/虚拟化";
  if (category === "sync") return "同步工具";
  if (category === "background_helper") return "后台辅助";
  if (category === "user_app") return "用户应用";
  return category || "未知";
}

function categoryTagType(category: string): "default" | "info" | "success" | "warning" | "error" {
  if (category === "system_critical" || category === "security") return "error";
  if (category === "system_service" || category === "windows_component") return "warning";
  if (category === "browser" || category === "user_app") return "info";
  if (category === "background_helper") return "success";
  return "default";
}

function riskTagType(risk: RiskLevel): "success" | "warning" | "error" {
  if (risk === "low") return "success";
  if (risk === "medium") return "warning";
  return "error";
}

function riskLabel(risk: RiskLevel) {
  if (risk === "low") return "低风险";
  if (risk === "medium") return "中风险";
  return "高风险";
}

function actionLabel(action: ProcessSuggestedAction) {
  if (action === "safe_to_end") return "可尝试结束";
  if (action === "end_after_save") return "先保存再结束";
  if (action === "review") return "先确认用途";
  return "避免结束";
}

function actionTagType(action: ProcessSuggestedAction): "default" | "success" | "warning" | "error" | "info" {
  if (action === "safe_to_end") return "success";
  if (action === "end_after_save") return "warning";
  if (action === "review") return "info";
  return "error";
}

function formatSource(source?: string | null) {
  if (!source) return "-";
  if (source === "local_rules") return "本地规则";
  if (source.startsWith("remote:")) return `远程模型：${source.slice("remote:".length)}`;
  return source;
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function formatCompactBytes(bytes: number) {
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(0)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

function formatRuntime(seconds: number) {
  if (seconds < 60) return `${seconds} 秒`;
  const minutes = Math.floor(seconds / 60);
  const remainSeconds = seconds % 60;
  if (minutes < 60) return remainSeconds > 0 ? `${minutes} 分 ${remainSeconds} 秒` : `${minutes} 分钟`;
  const hours = Math.floor(minutes / 60);
  const remainMinutes = minutes % 60;
  return remainMinutes > 0 ? `${hours} 小时 ${remainMinutes} 分` : `${hours} 小时`;
}

function formatTime(value: string) {
  return new Date(value).toLocaleTimeString("zh-CN", { hour12: false });
}

function buildChartOption(options: {
  labels: string[];
  data: number[];
  selectedLabel?: string;
  color: string;
  axisName: string;
  max?: number;
  tooltip: (value: number) => string;
  axisLabel?: (value: number) => string;
}) {
  return {
    animationDuration: 260,
    tooltip: {
      trigger: "axis",
      backgroundColor: "rgba(15, 23, 42, 0.92)",
      borderWidth: 0,
      textStyle: { color: "#f8fafc" },
      formatter: (params: Array<{ axisValue: string; data: number }>) => {
        const point = params[0];
        return [
          `<div style="font-weight:600;margin-bottom:4px;">${point.axisValue}</div>`,
          `<div>${options.tooltip(point.data)}</div>`,
          "<div style=\"margin-top:4px;color:#cbd5e1;\">点击曲线可回放该时刻</div>",
        ].join("");
      },
    },
    grid: { left: 48, right: 18, top: 20, bottom: 34 },
    xAxis: {
      type: "category",
      data: options.labels,
      boundaryGap: false,
      axisTick: { show: false },
      axisLine: { lineStyle: { color: "#d9e2f2" } },
      axisLabel: { color: "#64748b" },
    },
    yAxis: {
      type: "value",
      name: options.axisName,
      min: 0,
      max: options.max,
      splitNumber: 4,
      axisLabel: { color: "#64748b", formatter: options.axisLabel },
      splitLine: { lineStyle: { color: "#e9eef8" } },
    },
    series: [
      {
        type: "line",
        smooth: true,
        symbol: "circle",
        symbolSize: options.data.length <= 60 ? 7 : 5,
        showSymbol: options.data.length <= 120,
        data: options.data,
        lineStyle: { width: 3, color: options.color },
        itemStyle: { color: options.color, borderColor: "#ffffff", borderWidth: 2 },
        areaStyle: {
          color: {
            type: "linear",
            x: 0,
            y: 0,
            x2: 0,
            y2: 1,
            colorStops: [
              { offset: 0, color: `${options.color}44` },
              { offset: 1, color: `${options.color}08` },
            ],
          },
        },
        markLine: options.selectedLabel
          ? {
              symbol: "none",
              label: { show: false },
              lineStyle: { color: options.color, opacity: 0.45, width: 2, type: "dashed" },
              data: [{ xAxis: options.selectedLabel }],
            }
          : undefined,
      },
    ],
  };
}
</script>

<template>
  <n-space vertical :size="20" class="monitor-page">
    <n-card class="monitor-hero" :bordered="false">
      <div class="monitor-hero__content">
        <div class="monitor-hero__copy">
          <div class="monitor-hero__eyebrow">Process Replay</div>
          <h2 class="monitor-hero__title">{{ TEXT.title }}</h2>
          <n-text depth="3" class="monitor-hero__subtitle">{{ TEXT.subtitle }}</n-text>
        </div>
        <n-space class="monitor-hero__actions" wrap>
          <n-button
            type="primary"
            :disabled="monitoring"
            :loading="sampleLoading && monitoring"
            @click="startMonitoring"
          >
            {{ TEXT.startMonitor }}
          </n-button>
          <n-button secondary :disabled="!monitoring" @click="stopMonitoring">
            {{ TEXT.stopMonitor }}
          </n-button>
          <n-button
            secondary
            :loading="manualSampling || (sampleLoading && !monitoring)"
            @click="handleManualSample"
          >
            {{ TEXT.refreshNow }}
          </n-button>
          <n-button tertiary :disabled="samples.length === 0" @click="clearHistory">
            {{ TEXT.clearHistory }}
          </n-button>
          <n-tag round :type="monitoring ? 'success' : 'default'">
            {{ monitoring ? TEXT.sampling : TEXT.idle }}
          </n-tag>
          <n-text depth="3" class="monitor-hero__sample-tip">
            最近采样：{{ latestSampleTimeLabel }}
            <span v-if="manualSampling">，正在执行立即采样</span>
          </n-text>
        </n-space>
      </div>

      <div class="metrics-grid">
        <div class="metric-tile metric-tile--neutral">
          <div class="metric-tile__label">采样点数</div>
          <div class="metric-tile__value">{{ samples.length }}</div>
        </div>
        <div class="metric-tile metric-tile--cpu">
          <div class="metric-tile__label">当前 CPU</div>
          <div class="metric-tile__value">{{ latestCpuLabel }}</div>
          <div class="metric-tile__sub">峰值 {{ peakCpuLabel }}</div>
        </div>
        <div class="metric-tile metric-tile--memory">
          <div class="metric-tile__label">当前内存</div>
          <div class="metric-tile__value">{{ latestMemoryLabel }}</div>
          <div class="metric-tile__sub">峰值 {{ peakMemoryLabel }}</div>
        </div>
        <div class="metric-tile metric-tile--disk">
          <div class="metric-tile__label">当前磁盘</div>
          <div class="metric-tile__value">{{ latestDiskLabel }}</div>
          <div class="metric-tile__sub">峰值 {{ peakDiskLabel }}</div>
        </div>
        <div class="metric-tile metric-tile--selection metric-tile--wide">
          <div class="metric-tile__label">{{ TEXT.selectedTime }}</div>
          <div class="metric-tile__value">{{ selectedSampleTime }}</div>
          <div class="metric-tile__sub">{{ TEXT.chartHint }}</div>
        </div>
      </div>

      <n-alert v-if="error" type="error" :title="TEXT.monitoringFailed" style="margin-top: 16px">
        {{ error }}
      </n-alert>
    </n-card>

    <div class="chart-grid">
      <n-card class="chart-card chart-card--cpu" :bordered="false">
        <template #header>
          <div class="chart-card__header">
            <div>
              <div class="chart-card__title">{{ TEXT.chartCpuTitle }}</div>
              <div class="chart-card__desc">{{ TEXT.chartCpuDesc }}</div>
            </div>
            <n-tag round type="info">{{ latestCpuLabel }}</n-tag>
          </div>
        </template>
        <v-chart
          :option="cpuChartOption"
          class="chart-card__chart"
          autoresize
          @click="handleChartClick"
        />
      </n-card>

      <n-card class="chart-card chart-card--memory" :bordered="false">
        <template #header>
          <div class="chart-card__header">
            <div>
              <div class="chart-card__title">{{ TEXT.chartMemoryTitle }}</div>
              <div class="chart-card__desc">{{ TEXT.chartMemoryDesc }}</div>
            </div>
            <n-tag round type="success">{{ latestMemoryLabel }}</n-tag>
          </div>
        </template>
        <v-chart
          :option="memoryChartOption"
          class="chart-card__chart"
          autoresize
          @click="handleChartClick"
        />
      </n-card>

      <n-card class="chart-card chart-card--disk" :bordered="false">
        <template #header>
          <div class="chart-card__header">
            <div>
              <div class="chart-card__title">{{ TEXT.chartDiskTitle }}</div>
              <div class="chart-card__desc">{{ TEXT.chartDiskDesc }}</div>
            </div>
            <n-tag round type="warning">{{ latestDiskLabel }}</n-tag>
          </div>
        </template>
        <v-chart
          :option="diskChartOption"
          class="chart-card__chart"
          autoresize
          @click="handleChartClick"
        />
      </n-card>
    </div>

    <n-card :title="TEXT.anomalyTitle" :bordered="false">
      <n-space vertical :size="12">
        <n-empty v-if="anomalySamples.length === 0" :description="TEXT.anomalyEmpty" />
        <n-space v-else wrap :size="[10, 10]">
          <n-button
            v-for="item in anomalySamples"
            :key="item.index"
            secondary
            :type="selectedSampleIndex === item.index ? 'primary' : anomalySeverityType(item.severity)"
            @click="selectSample(item.index)"
          >
            {{ formatTime(item.sample.collectedAt) }} · {{ anomalyLabel(item.sample) }}
          </n-button>
        </n-space>
      </n-space>
    </n-card>

    <n-card :title="TEXT.snapshotTitle" :bordered="false">
      <n-space vertical :size="12">
        <n-space justify="space-between" align="center" wrap>
          <n-text depth="3">{{ TEXT.selectedTime }}：{{ selectedSampleTime }}</n-text>
          <n-space :wrap="false">
            <n-select v-model:value="sortBy" :options="sortOptions" style="width: 180px" />
            <n-button secondary @click="toggleSortDirection">
              {{ sortDirection === "desc" ? TEXT.sortOrderDesc : TEXT.sortOrderAsc }}
            </n-button>
          </n-space>
        </n-space>

        <n-input
          v-model:value="processQuery"
          clearable
          :placeholder="TEXT.queryPlaceholder"
        />

        <n-data-table
          v-if="sortedProcesses.length > 0"
          :columns="columns"
          :data="sortedProcesses"
          :scroll-x="1360"
          :max-height="620"
          size="small"
          :bordered="false"
        />
        <n-empty v-else :description="TEXT.snapshotEmpty" />
      </n-space>
    </n-card>

    <n-card
      v-if="sortedProcesses.some((item) => isProtectedProcess(item))"
      size="small"
      :bordered="false"
    >
      <n-alert type="warning" :title="TEXT.protected">
        {{ TEXT.protectedHint }}
      </n-alert>
    </n-card>

    <n-modal v-model:show="aiVisible" style="width: min(760px, calc(100vw - 32px))">
      <n-card :title="TEXT.aiDialogTitle" :bordered="false" size="small" role="dialog" aria-modal="true">
        <n-space vertical :size="12">
          <n-alert v-if="selectedProcessInsightError" type="error" :title="TEXT.aiFailed">
            {{ selectedProcessInsightError }}
          </n-alert>
          <template v-if="selectedProcess">
            <n-card size="small" embedded>
              <n-space vertical :size="8">
                <n-text depth="3">{{ TEXT.targetProcess }}</n-text>
                <div>{{ selectedProcess.name }} (PID {{ selectedProcess.pid }})</div>
                <n-text depth="3">{{ TEXT.executablePath }}</n-text>
                <div style="word-break: break-all">{{ selectedProcess.exePath || "-" }}</div>
                <n-text depth="3">{{ TEXT.commandLine }}</n-text>
                <div style="word-break: break-all">
                  {{ selectedProcess.command.join(" ") || "-" }}
                </div>
              </n-space>
            </n-card>
          </template>

          <template v-if="selectedProcessInsightPending">
            <n-text>{{ TEXT.pendingInsight }}</n-text>
          </template>

          <template v-else-if="selectedProcessInsight">
            <n-alert
              v-if="selectedProcessInsight.usedFallback"
              type="warning"
              :title="TEXT.fallbackTitle"
            >
              {{ selectedProcessInsight.fallbackReason || "-" }}
            </n-alert>
            <n-alert
              v-else-if="selectedProcessInsight.source.startsWith('remote:')"
              type="success"
              :title="TEXT.remoteOk"
            >
              {{ formatSource(selectedProcessInsight.source) }}
            </n-alert>
            <n-alert v-else type="info" :title="TEXT.localOnlyTitle">
              {{ formatSource(selectedProcessInsight.source) }}
            </n-alert>

            <n-space>
              <n-tag size="small" :type="riskTagType(selectedProcessInsight.risk)">
                {{ riskLabel(selectedProcessInsight.risk) }}
              </n-tag>
              <n-tag size="small" :type="actionTagType(selectedProcessInsight.suggestedAction)">
                {{ actionLabel(selectedProcessInsight.suggestedAction) }}
              </n-tag>
              <n-tag size="small" type="default">
                {{ formatSource(selectedProcessInsight.source) }}
              </n-tag>
            </n-space>

            <n-card size="small" embedded>
              <n-text depth="3">{{ TEXT.summary }}</n-text>
              <n-text style="display: block; margin-top: 8px; white-space: pre-wrap">
                {{ selectedProcessInsight.summary }}
              </n-text>
            </n-card>

            <n-card size="small" embedded>
              <n-text depth="3">{{ TEXT.reason }}</n-text>
              <n-text style="display: block; margin-top: 8px; white-space: pre-wrap">
                {{ selectedProcessInsight.reason }}
              </n-text>
            </n-card>

            <n-card size="small" embedded>
              <n-space vertical :size="12">
                <div>
                  <n-text depth="3">{{ TEXT.followUpTitle }}</n-text>
                  <n-text depth="3" style="display: block; margin-top: 6px">
                    {{ TEXT.followUpSubtitle }}
                  </n-text>
                </div>

                <n-space wrap :size="[8, 8]">
                  <n-button
                    v-for="question in suggestedFollowUpQuestions"
                    :key="question"
                    size="small"
                    secondary
                    @click="fillFollowUpQuestion(question)"
                  >
                    {{ question }}
                  </n-button>
                </n-space>

                <n-input
                  v-model:value="processFollowUpInput"
                  type="textarea"
                  :autosize="{ minRows: 2, maxRows: 5 }"
                  :placeholder="TEXT.followUpPlaceholder"
                />

                <n-space justify="end">
                  <n-button
                    type="primary"
                    :loading="followUpLoading"
                    :disabled="!selectedProcess"
                    @click="submitFollowUp()"
                  >
                    {{ TEXT.askFollowUp }}
                  </n-button>
                </n-space>

                <n-alert v-if="followUpError" type="error" :title="TEXT.followUpFailed">
                  {{ followUpError }}
                </n-alert>

                <n-empty
                  v-if="selectedProcessFollowUps.length === 0"
                  :description="TEXT.followUpEmpty"
                />

                <n-space v-else vertical :size="10">
                  <n-card
                    v-for="(item, index) in selectedProcessFollowUps"
                    :key="`${item.pid}-${index}`"
                    size="small"
                    embedded
                  >
                    <n-space vertical :size="8">
                      <div class="follow-up-item__question">{{ item.question }}</div>
                      <n-text style="white-space: pre-wrap">{{ item.answer }}</n-text>
                      <n-space>
                        <n-tag size="small" type="default">
                          {{ formatSource(item.source) }}
                        </n-tag>
                        <n-tag v-if="item.usedFallback" size="small" type="warning">
                          已回退本地规则
                        </n-tag>
                      </n-space>
                    </n-space>
                  </n-card>
                </n-space>
              </n-space>
            </n-card>
          </template>
        </n-space>

        <template #footer>
          <n-space justify="end">
            <n-button @click="aiVisible = false">{{ TEXT.close }}</n-button>
            <n-button
              type="primary"
              secondary
              :disabled="!selectedProcess"
              :loading="selectedProcess ? Boolean(processInsightPending[selectedProcess.pid]) : false"
              @click="retrySelectedProcessInsight"
            >
              {{ TEXT.retry }}
            </n-button>
          </n-space>
        </template>
      </n-card>
    </n-modal>

    <n-modal v-model:show="terminateVisible" style="width: min(560px, calc(100vw - 32px))">
      <n-card :title="TEXT.terminateDialogTitle" :bordered="false" size="small" role="dialog" aria-modal="true">
        <n-space vertical :size="12">
          <n-alert type="warning">{{ TEXT.terminateHint }}</n-alert>
          <div v-if="terminateTarget">
            <div>{{ terminateTarget.name }} (PID {{ terminateTarget.pid }})</div>
            <div style="margin-top: 8px; word-break: break-all">
              {{ terminateTarget.exePath || "-" }}
            </div>
          </div>
          <n-alert v-if="terminateError" type="error" :title="TEXT.terminateFailed">
            {{ terminateError }}
          </n-alert>
        </n-space>

        <template #footer>
          <n-space justify="end">
            <n-button @click="terminateVisible = false">{{ TEXT.close }}</n-button>
            <n-button
              type="error"
              :loading="terminating"
              :disabled="!terminateTarget"
              @click="confirmTerminate"
            >
              {{ TEXT.confirmTerminate }}
            </n-button>
          </n-space>
        </template>
      </n-card>
    </n-modal>
  </n-space>
</template>

<style scoped>
.monitor-page {
  padding-bottom: 12px;
}

.monitor-hero {
  overflow: hidden;
  background:
    radial-gradient(circle at top right, rgba(47, 109, 246, 0.14), transparent 28%),
    radial-gradient(circle at left bottom, rgba(30, 169, 124, 0.12), transparent 26%),
    linear-gradient(180deg, #fbfdff 0%, #f5f9ff 100%);
  box-shadow: 0 18px 48px rgba(15, 23, 42, 0.06);
}

.monitor-hero__content {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  align-items: flex-start;
  margin-bottom: 20px;
}

.monitor-hero__copy {
  display: flex;
  flex-direction: column;
  gap: 8px;
  max-width: 720px;
}

.monitor-hero__eyebrow {
  color: #5b6b85;
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.12em;
  text-transform: uppercase;
}

.monitor-hero__title {
  margin: 0;
  color: #0f172a;
  font-size: 28px;
  line-height: 1.1;
}

.monitor-hero__subtitle {
  max-width: 62ch;
}

.monitor-hero__actions {
  justify-content: flex-end;
}

.monitor-hero__sample-tip {
  display: inline-flex;
  align-items: center;
  min-height: 34px;
  padding: 0 4px;
}

.metrics-grid {
  display: grid;
  grid-template-columns: repeat(5, minmax(0, 1fr));
  gap: 12px;
}

.metric-tile {
  min-height: 104px;
  padding: 16px;
  border-radius: 18px;
  border: 1px solid rgba(148, 163, 184, 0.18);
  background: rgba(255, 255, 255, 0.82);
  backdrop-filter: blur(10px);
}

.metric-tile--neutral {
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.94), rgba(248, 250, 252, 0.96));
}

.metric-tile--cpu {
  background: linear-gradient(180deg, rgba(240, 246, 255, 0.98), rgba(228, 238, 255, 0.94));
}

.metric-tile--memory {
  background: linear-gradient(180deg, rgba(239, 251, 246, 0.98), rgba(227, 247, 239, 0.94));
}

.metric-tile--disk {
  background: linear-gradient(180deg, rgba(255, 247, 237, 0.98), rgba(255, 239, 220, 0.94));
}

.metric-tile--selection {
  background: linear-gradient(180deg, rgba(247, 250, 255, 0.98), rgba(236, 242, 252, 0.94));
}

.metric-tile--wide {
  grid-column: span 2;
}

.metric-tile__label {
  color: #64748b;
  font-size: 13px;
}

.metric-tile__value {
  margin-top: 10px;
  color: #0f172a;
  font-size: 28px;
  font-weight: 700;
  line-height: 1.1;
}

.metric-tile__sub {
  margin-top: 10px;
  color: #506178;
  font-size: 12px;
}

.chart-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 16px;
}

.chart-card {
  height: 100%;
  border-radius: 20px;
  overflow: hidden;
  box-shadow: 0 16px 36px rgba(15, 23, 42, 0.06);
}

.chart-card--cpu {
  background: linear-gradient(180deg, #ffffff 0%, #f3f7ff 100%);
}

.chart-card--memory {
  background: linear-gradient(180deg, #ffffff 0%, #f2fcf8 100%);
}

.chart-card--disk {
  background: linear-gradient(180deg, #ffffff 0%, #fff7ed 100%);
}

.chart-card__header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 12px;
}

.chart-card__title {
  color: #0f172a;
  font-size: 16px;
  font-weight: 700;
}

.chart-card__desc {
  margin-top: 4px;
  color: #64748b;
  font-size: 12px;
  line-height: 1.5;
}

.chart-card__chart {
  height: 290px;
}

.follow-up-item__question {
  color: #0f172a;
  font-weight: 600;
}

@media (max-width: 1100px) {
  .chart-grid {
    grid-template-columns: 1fr;
  }

  .metrics-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .metric-tile--wide {
    grid-column: span 2;
  }
}

@media (max-width: 900px) {
  .monitor-hero__content {
    flex-direction: column;
  }

  .monitor-hero__actions {
    justify-content: flex-start;
  }
}

@media (max-width: 640px) {
  .metrics-grid {
    grid-template-columns: 1fr;
  }

  .metric-tile--wide {
    grid-column: span 1;
  }

  .metric-tile__value {
    font-size: 24px;
  }
}
</style>
