<script setup lang="ts">
import { computed, h, onMounted, ref } from "vue";
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
import { useProcesses } from "@/composables/useProcesses";
import { useAppStore } from "@/stores/app";
import type {
  ProcessAiInsight,
  ProcessRecord,
  ProcessSuggestedAction,
  RiskLevel,
} from "@/types";

type ProcessSortMode = "score" | "cpu" | "memory" | "disk" | "runtime" | "name";
type SortDirection = "desc" | "asc";

const TEXT = {
  title: "进程诊断",
  refresh: "刷新列表",
  queryPlaceholder: "按进程名、路径或命令行搜索",
  categoryPlaceholder: "按进程类别筛选",
  sortPlaceholder: "排序方式",
  sortOrderDesc: "从大到小",
  sortOrderAsc: "从小到大",
  sortByScore: "按综合资源压力",
  sortByCpu: "按 CPU",
  sortByMemory: "按内存",
  sortByDisk: "按磁盘活动",
  sortByRuntime: "按运行时长",
  sortByName: "按进程名",
  allCategories: "全部类别",
  noData: "当前没有可展示的进程数据",
  loadFailed: "进程列表加载失败",
  aiFailed: "AI 诊断失败",
  terminateFailed: "结束进程失败",
  pid: "PID",
  name: "进程名",
  category: "类别",
  cpu: "CPU",
  memory: "内存",
  disk: "磁盘活动",
  status: "状态",
  score: "资源压力",
  runtime: "运行时长",
  actions: "操作",
  aiInspect: "AI 诊断",
  terminate: "结束进程",
  protected: "受保护",
  aiDialogTitle: "AI 进程解读",
  terminateDialogTitle: "确认结束进程",
  close: "关闭",
  confirmTerminate: "确认结束",
  retry: "重新诊断",
  targetProcess: "目标进程",
  executablePath: "可执行文件",
  commandLine: "命令行",
  remoteOk: "已调用远程 AI 模型",
  fallbackTitle: "远程 AI 调用失败，已回退到本地规则",
  localOnlyTitle: "当前显示的是本地规则分析结果",
  summary: "结论",
  reason: "原因说明",
  fallbackReason: "回退原因",
  terminateHint: "该操作会立即结束所选进程，请先确认没有未保存工作。",
  protectedHint: "关键系统或安全进程已被禁止结束。",
  countLabel: "进程数",
  topCpuLabel: "最高 CPU",
  topMemoryLabel: "最高内存",
  protectedCountLabel: "受保护进程",
};

const store = useAppStore();
const message = useMessage();
const {
  processes,
  loading,
  error,
  terminating,
  terminateError,
  loadProcesses,
  requestProcessInsight,
  terminateProcess,
} = useProcesses();

const query = ref("");
const selectedCategory = ref<string>("all");
const sortBy = ref<ProcessSortMode>("score");
const sortDirection = ref<SortDirection>("desc");

const aiVisible = ref(false);
const selectedProcess = ref<ProcessRecord | null>(null);
const processInsightCache = ref<Record<number, ProcessAiInsight>>({});
const processInsightPending = ref<Record<number, boolean>>({});
const processInsightErrors = ref<Record<number, string>>({});

const terminateVisible = ref(false);
const terminateTarget = ref<ProcessRecord | null>(null);

const tableScrollX = 1320;

onMounted(async () => {
  await refreshProcesses();
});

const categoryOptions = computed(() => {
  const values = Array.from(new Set(processes.value.map((item) => item.category))).sort();
  return [
    { label: TEXT.allCategories, value: "all" },
    ...values.map((value) => ({
      label: categoryLabel(value),
      value,
    })),
  ];
});

const sortOptions = [
  { label: TEXT.sortByScore, value: "score" },
  { label: TEXT.sortByCpu, value: "cpu" },
  { label: TEXT.sortByMemory, value: "memory" },
  { label: TEXT.sortByDisk, value: "disk" },
  { label: TEXT.sortByRuntime, value: "runtime" },
  { label: TEXT.sortByName, value: "name" },
];

const filteredProcesses = computed(() => {
  const keyword = query.value.trim().toLowerCase();
  return processes.value.filter((item) => {
    const matchedCategory =
      selectedCategory.value === "all" || item.category === selectedCategory.value;
    if (!matchedCategory) {
      return false;
    }

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

const topCpuUsage = computed(() =>
  filteredProcesses.value.length > 0
    ? `${Math.max(...filteredProcesses.value.map((item) => item.cpuUsage)).toFixed(1)}%`
    : "-"
);

const topMemoryUsage = computed(() =>
  filteredProcesses.value.length > 0
    ? formatBytes(Math.max(...filteredProcesses.value.map((item) => item.memoryBytes)))
    : "-"
);

const protectedCount = computed(
  () => filteredProcesses.value.filter((item) => isProtectedProcess(item)).length
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

const columns = computed<DataTableColumns<ProcessRecord>>(() => [
  {
    title: TEXT.name,
    key: "name",
    minWidth: 230,
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
  {
    title: TEXT.pid,
    key: "pid",
    width: 90,
  },
  {
    title: TEXT.category,
    key: "category",
    width: 132,
    render: (row) =>
      h(NTag, { size: "small", type: categoryTagType(row.category) }, () =>
        categoryLabel(row.category)
      ),
  },
  {
    title: TEXT.cpu,
    key: "cpuUsage",
    width: 96,
    render: (row) => `${row.cpuUsage.toFixed(1)}%`,
  },
  {
    title: TEXT.memory,
    key: "memoryBytes",
    width: 110,
    render: (row) => formatBytes(row.memoryBytes),
  },
  {
    title: TEXT.disk,
    key: "disk",
    width: 124,
    render: (row) => `${formatBytes(row.diskReadBytes + row.diskWrittenBytes)}/s`,
  },
  {
    title: TEXT.score,
    key: "resourceScore",
    width: 100,
    render: (row) => row.resourceScore.toFixed(1),
  },
  {
    title: TEXT.runtime,
    key: "runTimeSeconds",
    width: 120,
    render: (row) => formatRuntime(row.runTimeSeconds),
  },
  {
    title: TEXT.status,
    key: "status",
    width: 96,
    render: (row) => row.status || "running",
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

async function refreshProcesses() {
  await loadProcesses(40);
}

function processActionButtonText(pid: number): string {
  if (processInsightPending.value[pid]) {
    return "解读中";
  }
  if (processInsightCache.value[pid]) {
    return "查看解读";
  }
  if (processInsightErrors.value[pid]) {
    return "重试解读";
  }
  return TEXT.aiInspect;
}

async function queueProcessInsight(process: ProcessRecord, force = false) {
  if (processInsightPending.value[process.pid]) {
    return;
  }
  if (!force && processInsightCache.value[process.pid]) {
    return;
  }

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
  if (!selectedProcess.value) {
    return;
  }

  void queueProcessInsight(selectedProcess.value, true);
  message.info("已重新加入后台解读队列。");
}

function openTerminateDialog(process: ProcessRecord) {
  terminateTarget.value = process;
  terminateVisible.value = true;
}

async function confirmTerminate() {
  if (!terminateTarget.value) {
    return;
  }

  const result = await terminateProcess(terminateTarget.value.pid);
  if (!result) {
    return;
  }

  message.success(result);
  terminateVisible.value = false;
  terminateTarget.value = null;
  await refreshProcesses();
}

function compareProcesses(
  left: ProcessRecord,
  right: ProcessRecord,
  mode: ProcessSortMode
): number {
  if (mode === "cpu") {
    return left.cpuUsage - right.cpuUsage;
  }
  if (mode === "memory") {
    return left.memoryBytes - right.memoryBytes;
  }
  if (mode === "disk") {
    return (
      left.diskReadBytes +
      left.diskWrittenBytes -
      (right.diskReadBytes + right.diskWrittenBytes)
    );
  }
  if (mode === "runtime") {
    return left.runTimeSeconds - right.runTimeSeconds;
  }
  if (mode === "name") {
    return left.name.localeCompare(right.name);
  }
  return left.resourceScore - right.resourceScore;
}

function toggleSortDirection() {
  sortDirection.value = sortDirection.value === "desc" ? "asc" : "desc";
}

function isProtectedProcess(process: ProcessRecord): boolean {
  return (
    process.isCritical ||
    process.category === "system_critical" ||
    process.category === "security"
  );
}

function categoryLabel(category: string): string {
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

function riskLabel(risk: RiskLevel): string {
  if (risk === "low") return "低风险";
  if (risk === "medium") return "中风险";
  return "高风险";
}

function actionLabel(action: ProcessSuggestedAction): string {
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

function formatSource(source?: string | null): string {
  if (!source) return "-";
  if (source === "local_rules") return "本地规则";
  if (source.startsWith("remote:")) {
    return `远程模型：${source.slice("remote:".length)}`;
  }
  return source;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function formatRuntime(seconds: number): string {
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
</script>

<template>
  <n-space vertical :size="20">
    <n-card :title="TEXT.title">
      <n-space vertical :size="16">
        <n-grid :cols="4" :x-gap="12">
          <n-gi>
            <n-statistic :label="TEXT.countLabel" :value="filteredProcesses.length" />
          </n-gi>
          <n-gi>
            <n-statistic :label="TEXT.topCpuLabel" :value="topCpuUsage" />
          </n-gi>
          <n-gi>
            <n-statistic :label="TEXT.topMemoryLabel" :value="topMemoryUsage" />
          </n-gi>
          <n-gi>
            <n-statistic :label="TEXT.protectedCountLabel" :value="protectedCount" />
          </n-gi>
        </n-grid>

        <n-grid :cols="4" :x-gap="12">
          <n-gi :span="2">
            <n-input
              v-model:value="query"
              clearable
              :placeholder="TEXT.queryPlaceholder"
            />
          </n-gi>
          <n-gi>
            <n-select
              v-model:value="selectedCategory"
              :options="categoryOptions"
              :placeholder="TEXT.categoryPlaceholder"
            />
          </n-gi>
          <n-gi>
            <n-space :wrap="false">
              <n-select
                v-model:value="sortBy"
                :options="sortOptions"
                :placeholder="TEXT.sortPlaceholder"
              />
              <n-button secondary @click="toggleSortDirection">
                {{ sortDirection === "desc" ? TEXT.sortOrderDesc : TEXT.sortOrderAsc }}
              </n-button>
            </n-space>
          </n-gi>
        </n-grid>

        <n-space justify="end">
          <n-button
            type="primary"
            secondary
            :loading="loading"
            @click="refreshProcesses"
          >
            {{ TEXT.refresh }}
          </n-button>
        </n-space>

        <n-alert v-if="error" type="error" :title="TEXT.loadFailed">
          {{ error }}
        </n-alert>

        <n-data-table
          v-if="sortedProcesses.length > 0"
          :columns="columns"
          :data="sortedProcesses"
          :loading="loading"
          :max-height="620"
          :scroll-x="tableScrollX"
          size="small"
          :bordered="false"
        />

        <n-empty
          v-else-if="!loading"
          :description="TEXT.noData"
        />
      </n-space>
    </n-card>

    <n-card v-if="filteredProcesses.some((item) => isProtectedProcess(item))" size="small">
      <n-alert type="warning" :title="TEXT.protected">
        {{ TEXT.protectedHint }}
      </n-alert>
    </n-card>

    <n-modal v-model:show="aiVisible" style="width: min(760px, calc(100vw - 32px))">
      <n-card
        :title="TEXT.aiDialogTitle"
        :bordered="false"
        size="small"
        role="dialog"
        aria-modal="true"
      >
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
                <n-text depth="3">
                  {{ TEXT.runtime }}：{{ formatRuntime(selectedProcess.runTimeSeconds) }}
                </n-text>
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

            <n-card
              v-if="selectedProcessInsight.usedFallback && selectedProcessInsight.fallbackReason"
              size="small"
              embedded
            >
              <n-text depth="3">{{ TEXT.fallbackReason }}</n-text>
              <n-text style="display: block; margin-top: 8px; white-space: pre-wrap">
                {{ selectedProcessInsight.fallbackReason }}
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
      <n-card
        :title="TEXT.terminateDialogTitle"
        :bordered="false"
        size="small"
        role="dialog"
        aria-modal="true"
      >
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
