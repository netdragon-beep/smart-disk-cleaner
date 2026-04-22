<script setup lang="ts">
import { computed, h, onBeforeUnmount, onMounted, ref } from "vue";
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
  useMessage,
  type DataTableColumns,
} from "naive-ui";
import VChart from "vue-echarts";
import { use } from "echarts/core";
import { LineChart } from "echarts/charts";
import {
  GridComponent,
  LegendComponent,
  TooltipComponent,
} from "echarts/components";
import { CanvasRenderer } from "echarts/renderers";
import { useProcesses } from "@/composables/useProcesses";
import { useAppStore } from "@/stores/app";
import type {
  ProcessAiInsight,
  ProcessMonitorSnapshot,
  ProcessRecord,
  ProcessSuggestedAction,
  RiskLevel,
} from "@/types";

use([LineChart, GridComponent, LegendComponent, TooltipComponent, CanvasRenderer]);

type ProcessSortMode = "score" | "cpu" | "memory" | "disk" | "runtime" | "name";
type SortDirection = "desc" | "asc";

const TEXT = {
  title: "卡顿监控",
  subtitle: "持续采样最近一段时间的 CPU、内存和磁盘压力，帮助回放卡顿发生时的进程状况。",
  startMonitor: "开始监控",
  stopMonitor: "停止监控",
  clearHistory: "清空记录",
  sampling: "采样中",
  idle: "未监控",
  refreshNow: "立即采样",
  sampleCount: "采样点数",
  latestCpu: "最新 CPU",
  latestMemory: "最新内存",
  latestDisk: "最新磁盘",
  chartTitle: "最近监控曲线",
  chartHint: "点击图表上的采样点，可回放当时的进程快照。",
  anomalyTitle: "异常时刻",
  anomalyEmpty: "当前还没有明显异常时刻，可以先开始监控并等待卡顿复现。",
  snapshotTitle: "时刻进程快照",
  snapshotEmpty: "还没有采样数据。",
  selectedTime: "当前时刻",
  queryPlaceholder: "按进程名、路径或命令行搜索",
  sortByScore: "按综合资源压力",
  sortByCpu: "按 CPU",
  sortByMemory: "按内存",
  sortByDisk: "按磁盘活动",
  sortByRuntime: "按运行时长",
  sortByName: "按进程名",
  sortOrderDesc: "从大到小",
  sortOrderAsc: "从小到大",
  aiInspect: "AI 诊断",
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
  status: "状态",
  actions: "操作",
  aiDialogTitle: "AI 进程解读",
  aiFailed: "AI 诊断失败",
  remoteOk: "已调用远程 AI 模型",
  fallbackTitle: "远程 AI 调用失败，已回退到本地规则",
  localOnlyTitle: "当前显示的是本地规则分析结果",
  summary: "结论",
  reason: "原因说明",
  fallbackReason: "回退原因",
  close: "关闭",
  retry: "重新诊断",
  terminateDialogTitle: "确认结束进程",
  terminateHint: "该操作会立即结束所选进程，请先确认没有未保存工作。",
  terminateFailed: "结束进程失败",
  confirmTerminate: "确认结束",
  protectedHint: "关键系统或安全进程已被禁止结束。",
};

const MONITOR_INTERVAL_MS = 1000;
const MAX_SAMPLES = 300;
const TOP_PROCESS_LIMIT = 12;
const DISK_ALERT_BYTES = 20 * 1024 * 1024;

const store = useAppStore();
const message = useMessage();
const {
  error,
  terminating,
  terminateError,
  requestProcessInsight,
  loadMonitorSnapshot,
  terminateProcess,
} = useProcesses();

const monitoring = ref(false);
const sampleLoading = ref(false);
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

const terminateVisible = ref(false);
const terminateTarget = ref<ProcessRecord | null>(null);

let monitorTimer: ReturnType<typeof setInterval> | null = null;

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
  if (samples.value.length === 0) {
    return null;
  }
  if (selectedSampleIndex.value === null) {
    return samples.value[samples.value.length - 1];
  }
  return samples.value[selectedSampleIndex.value] ?? samples.value[samples.value.length - 1];
});

const latestMemoryPercent = computed(() =>
  latestSample.value ? memoryUsagePercent(latestSample.value).toFixed(1) + "%" : "-"
);

const latestDiskLabel = computed(() =>
  latestSample.value ? `${formatBytes(latestSample.value.diskBytesPerSec)}/s` : "-"
);

const filteredProcesses = computed(() => {
  const snapshot = selectedSample.value;
  if (!snapshot) {
    return [];
  }
  const keyword = processQuery.value.trim().toLowerCase();
  return snapshot.topProcesses.filter((item) => {
    if (!keyword) {
      return true;
    }
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
    if (result !== 0) {
      return result * multiplier;
    }
    return left.name.localeCompare(right.name);
  });
  return rows;
});

const chartOption = computed(() => {
  const xAxis = samples.value.map((item) => formatTime(item.collectedAt));
  return {
    tooltip: {
      trigger: "axis",
    },
    legend: {
      top: 0,
    },
    grid: {
      left: 48,
      right: 18,
      top: 44,
      bottom: 36,
    },
    xAxis: {
      type: "category",
      data: xAxis,
      boundaryGap: false,
    },
    yAxis: [
      {
        type: "value",
        name: "%",
        min: 0,
        max: 100,
      },
      {
        type: "value",
        name: "磁盘 MB/s",
        min: 0,
      },
    ],
    series: [
      {
        name: "CPU",
        type: "line",
        smooth: true,
        showSymbol: false,
        data: samples.value.map((item) => Number(item.systemCpuUsage.toFixed(1))),
      },
      {
        name: "内存",
        type: "line",
        smooth: true,
        showSymbol: false,
        data: samples.value.map((item) => Number(memoryUsagePercent(item).toFixed(1))),
      },
      {
        name: "磁盘",
        type: "line",
        smooth: true,
        showSymbol: false,
        yAxisIndex: 1,
        data: samples.value.map((item) =>
          Number((item.diskBytesPerSec / (1024 * 1024)).toFixed(2))
        ),
      },
    ],
  };
});

const anomalySamples = computed(() => {
  return samples.value
    .map((sample, index) => {
      const cpuScore = sample.systemCpuUsage / 85;
      const memoryScore = memoryUsagePercent(sample) / 85;
      const diskScore = sample.diskBytesPerSec / DISK_ALERT_BYTES;
      const severity = Math.max(cpuScore, memoryScore, diskScore);
      return {
        index,
        sample,
        severity,
      };
    })
    .filter((item) => item.severity >= 1)
    .sort((left, right) => right.severity - left.severity)
    .slice(0, 12);
});

const selectedProcessInsight = computed(() =>
  selectedProcess.value ? processInsightCache.value[selectedProcess.value.pid] ?? null : null
);
const selectedProcessInsightPending = computed(() =>
  selectedProcess.value ? Boolean(processInsightPending.value[selectedProcess.value.pid]) : false
);
const selectedProcessInsightError = computed(() =>
  selectedProcess.value ? processInsightErrors.value[selectedProcess.value.pid] ?? null : null
);

const columns = computed<DataTableColumns<ProcessRecord>>(() => [
  {
    title: TEXT.name,
    key: "name",
    minWidth: 220,
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
  {
    title: TEXT.cpu,
    key: "cpuUsage",
    width: 90,
    render: (row) => `${row.cpuUsage.toFixed(1)}%`,
  },
  {
    title: TEXT.memory,
    key: "memoryBytes",
    width: 108,
    render: (row) => formatBytes(row.memoryBytes),
  },
  {
    title: TEXT.disk,
    key: "disk",
    width: 118,
    render: (row) => `${formatBytes(row.diskReadBytes + row.diskWrittenBytes)}/s`,
  },
  {
    title: TEXT.score,
    key: "resourceScore",
    width: 96,
    render: (row) => row.resourceScore.toFixed(1),
  },
  {
    title: TEXT.runtime,
    key: "runTimeSeconds",
    width: 116,
    render: (row) => formatRuntime(row.runTimeSeconds),
  },
  {
    title: TEXT.actions,
    key: "actions",
    width: 188,
    fixed: "right",
    render: (row) =>
      h(
        "div",
        {
          style:
            "display: flex; justify-content: flex-end; gap: 8px; flex-wrap: nowrap;",
        },
        [
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
        ]
      ),
  },
]);

onMounted(async () => {
  await sampleOnce();
});

onBeforeUnmount(() => {
  stopMonitoring();
});

async function sampleOnce() {
  if (sampleLoading.value) {
    return;
  }
  sampleLoading.value = true;

  try {
    const snapshot = await loadMonitorSnapshot(TOP_PROCESS_LIMIT);
    if (!snapshot) {
      return;
    }

    const wasViewingLatest =
      selectedSampleIndex.value === null ||
      selectedSampleIndex.value >= samples.value.length - 1;

    const next = [...samples.value, snapshot];
    if (next.length > MAX_SAMPLES) {
      next.splice(0, next.length - MAX_SAMPLES);
    }
    samples.value = next;

    if (wasViewingLatest) {
      selectedSampleIndex.value = next.length - 1;
    }
  } finally {
    sampleLoading.value = false;
  }
}

async function startMonitoring() {
  if (monitoring.value) {
    return;
  }
  monitoring.value = true;
  await sampleOnce();
  monitorTimer = setInterval(() => {
    void sampleOnce();
  }, MONITOR_INTERVAL_MS);
}

function stopMonitoring() {
  monitoring.value = false;
  if (monitorTimer) {
    clearInterval(monitorTimer);
    monitorTimer = null;
  }
}

function clearHistory() {
  samples.value = [];
  selectedSampleIndex.value = null;
}

function selectSample(index: number) {
  selectedSampleIndex.value = index;
}

function handleChartClick(params: { dataIndex?: number }) {
  if (typeof params.dataIndex === "number") {
    selectSample(params.dataIndex);
  }
}

function processActionButtonText(pid: number): string {
  if (processInsightPending.value[pid]) return "解读中";
  if (processInsightCache.value[pid]) return "查看解读";
  if (processInsightErrors.value[pid]) return "重试解读";
  return TEXT.aiInspect;
}

async function queueProcessInsight(process: ProcessRecord, force = false) {
  if (processInsightPending.value[process.pid]) return;
  if (!force && processInsightCache.value[process.pid]) return;

  processInsightPending.value = {
    ...processInsightPending.value,
    [process.pid]: true,
  };

  const nextErrors = { ...processInsightErrors.value };
  delete nextErrors[process.pid];
  processInsightErrors.value = nextErrors;

  try {
    const result = await requestProcessInsight(process.pid, store.config);
    processInsightCache.value = {
      ...processInsightCache.value,
      [process.pid]: result,
    };
  } catch (error: any) {
    processInsightErrors.value = {
      ...processInsightErrors.value,
      [process.pid]: typeof error === "string" ? error : error?.message || String(error),
    };
  } finally {
    processInsightPending.value = {
      ...processInsightPending.value,
      [process.pid]: false,
    };
  }
}

async function handleProcessAiAction(process: ProcessRecord) {
  selectedProcess.value = process;
  if (processInsightCache.value[process.pid] || processInsightErrors.value[process.pid]) {
    aiVisible.value = true;
    return;
  }
  if (processInsightPending.value[process.pid]) {
    message.info("这个进程正在后台解读，稍后再点查看结果。");
    return;
  }
  void queueProcessInsight(process);
  message.info("已开始后台解读，你可以继续查看其它项目。");
}

async function retrySelectedProcessInsight() {
  if (!selectedProcess.value) return;
  void queueProcessInsight(selectedProcess.value, true);
  message.info("已重新加入后台解读队列。");
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
    return (
      left.diskReadBytes +
      left.diskWrittenBytes -
      (right.diskReadBytes + right.diskWrittenBytes)
    );
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
  if (sample.diskBytesPerSec >= DISK_ALERT_BYTES) {
    reasons.push(`磁盘 ${formatBytes(sample.diskBytesPerSec)}/s`);
  }
  return reasons.join(" / ") || "资源异常";
}

function isProtectedProcess(process: ProcessRecord) {
  return (
    process.isCritical ||
    process.category === "system_critical" ||
    process.category === "security"
  );
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

function categoryTagType(
  category: string
): "default" | "info" | "success" | "warning" | "error" {
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

function actionTagType(
  action: ProcessSuggestedAction
): "default" | "success" | "warning" | "error" | "info" {
  if (action === "safe_to_end") return "success";
  if (action === "end_after_save") return "warning";
  if (action === "review") return "info";
  return "error";
}

function formatSource(source?: string | null) {
  if (!source) return "-";
  if (source === "local_rules") return "本地规则";
  if (source.startsWith("remote:")) {
    return `远程模型：${source.slice("remote:".length)}`;
  }
  return source;
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function formatRuntime(seconds: number) {
  if (seconds < 60) return `${seconds} 秒`;
  const minutes = Math.floor(seconds / 60);
  const remainSeconds = seconds % 60;
  if (minutes < 60) {
    return remainSeconds > 0 ? `${minutes} 分 ${remainSeconds} 秒` : `${minutes} 分钟`;
  }
  const hours = Math.floor(minutes / 60);
  const remainMinutes = minutes % 60;
  return remainMinutes > 0 ? `${hours} 小时 ${remainMinutes} 分` : `${hours} 小时`;
}

function formatTime(value: string) {
  const date = new Date(value);
  return date.toLocaleTimeString("zh-CN", { hour12: false });
}
</script>

<template>
  <n-space vertical :size="20">
    <n-card :title="TEXT.title">
      <n-space vertical :size="14">
        <n-text depth="3">{{ TEXT.subtitle }}</n-text>

        <n-space>
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
          <n-button secondary :loading="sampleLoading && !monitoring" @click="sampleOnce">
            {{ TEXT.refreshNow }}
          </n-button>
          <n-button tertiary :disabled="samples.length === 0" @click="clearHistory">
            {{ TEXT.clearHistory }}
          </n-button>
          <n-tag :type="monitoring ? 'success' : 'default'">
            {{ monitoring ? TEXT.sampling : TEXT.idle }}
          </n-tag>
        </n-space>

        <n-alert v-if="error" type="error" title="监控采样失败">
          {{ error }}
        </n-alert>

        <n-grid :cols="4" :x-gap="12">
          <n-gi>
            <n-statistic :label="TEXT.sampleCount" :value="samples.length" />
          </n-gi>
          <n-gi>
            <n-statistic
              :label="TEXT.latestCpu"
              :value="latestSample ? `${latestSample.systemCpuUsage.toFixed(1)}%` : '-'"
            />
          </n-gi>
          <n-gi>
            <n-statistic :label="TEXT.latestMemory" :value="latestMemoryPercent" />
          </n-gi>
          <n-gi>
            <n-statistic :label="TEXT.latestDisk" :value="latestDiskLabel" />
          </n-gi>
        </n-grid>
      </n-space>
    </n-card>

    <n-card :title="TEXT.chartTitle">
      <n-space vertical :size="12">
        <n-text depth="3">{{ TEXT.chartHint }}</n-text>
        <v-chart
          :option="chartOption"
          style="height: 320px"
          autoresize
          @click="handleChartClick"
        />
      </n-space>
    </n-card>

    <n-card :title="TEXT.anomalyTitle">
      <n-space vertical :size="12">
        <n-empty v-if="anomalySamples.length === 0" :description="TEXT.anomalyEmpty" />
        <n-space v-else wrap :size="[10, 10]">
          <n-button
            v-for="item in anomalySamples"
            :key="item.index"
            secondary
            :type="selectedSampleIndex === item.index ? 'primary' : 'warning'"
            @click="selectSample(item.index)"
          >
            {{ formatTime(item.sample.collectedAt) }} · {{ anomalyLabel(item.sample) }}
          </n-button>
        </n-space>
      </n-space>
    </n-card>

    <n-card :title="TEXT.snapshotTitle">
      <n-space vertical :size="12">
        <n-space justify="space-between" align="center">
          <n-text depth="3">
            {{ TEXT.selectedTime }}：{{ selectedSample ? formatTime(selectedSample.collectedAt) : "-" }}
          </n-text>
          <n-space :wrap="false">
            <n-select
              v-model:value="sortBy"
              :options="sortOptions"
              style="width: 180px"
            />
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
          :scroll-x="1320"
          :max-height="620"
          size="small"
          :bordered="false"
        />
        <n-empty v-else :description="TEXT.snapshotEmpty" />
      </n-space>
    </n-card>

    <n-card v-if="sortedProcesses.some((item) => isProtectedProcess(item))" size="small">
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
                <n-text depth="3">目标进程</n-text>
                <div>{{ selectedProcess.name }} (PID {{ selectedProcess.pid }})</div>
                <n-text depth="3">可执行文件</n-text>
                <div style="word-break: break-all">{{ selectedProcess.exePath || "-" }}</div>
                <n-text depth="3">命令行</n-text>
                <div style="word-break: break-all">
                  {{ selectedProcess.command.join(" ") || "-" }}
                </div>
              </n-space>
            </n-card>
          </template>

          <template v-if="selectedProcessInsightPending">
            <n-text>后台正在解读这个进程，你可以先关闭窗口继续操作，稍后再回来查看结果。</n-text>
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

            <n-alert
              v-else
              type="info"
              :title="TEXT.localOnlyTitle"
            >
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
          <n-alert type="warning">
            {{ TEXT.terminateHint }}
          </n-alert>
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
