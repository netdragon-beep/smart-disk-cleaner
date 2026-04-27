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
  NList,
  NListItem,
  useMessage,
  type DataTableColumns,
  type DataTableRowKey,
} from "naive-ui";
import { open } from "@tauri-apps/plugin-dialog";
import { useAppStore } from "@/stores/app";
import { useCleanup } from "@/composables/useCleanup";
import { useProcesses } from "@/composables/useProcesses";
import type { BlockingProcessInfo, FileSuggestion, OperationLogEntry } from "@/types";

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
  browse: "\u6D4F\u89C8",
  dryRun: "\u6A21\u62DF\u8FD0\u884C",
  chooseFiles: "\u9009\u62E9\u6587\u4EF6",
  protectedNotice: "\u5176\u4E2D\u90E8\u5206\u6587\u4EF6\u88AB\u6807\u8BB0\u4E3A\u201C\u4FDD\u7559\u201D\u6216\u201C\u5F85\u5BA1\u201D\uFF0C\u5DF2\u7981\u6B62\u5728\u8FD9\u91CC\u76F4\u63A5\u6267\u884C\u6E05\u7406\u3002",
  executeCleanup: "\u6267\u884C\u6E05\u7406",
  executeMove: "\u6267\u884C\u79FB\u52A8",
  selected: "\u5DF2\u9009",
  item: "\u9879",
  error: "\u9519\u8BEF",
  confirmCleanup: "\u786E\u8BA4\u6E05\u7406",
  confirmMove: "\u786E\u8BA4\u79FB\u52A8",
  confirmMoveAction: "\u786E\u8BA4\u79FB\u52A8",
  simulate: "\u6A21\u62DF",
  execute: "\u6267\u884C",
  recycle: "\u56DE\u6536",
  cancel: "\u53D6\u6D88",
  confirmExecute: "\u786E\u8BA4\u6267\u884C",
  simulateRun: "\u6A21\u62DF\u8FD0\u884C",
  fileQuestionSuffix: "\u4E2A\u6587\u4EF6\uFF1F",
  realChangeWarning: "\u8FD9\u5C06\u5BF9\u60A8\u7684\u6587\u4EF6\u7CFB\u7EDF\u8FDB\u884C\u771F\u5B9E\u66F4\u6539\uFF01",
  executionResult: "\u6267\u884C\u7ED3\u679C",
  moveResult: "\u79FB\u52A8\u7ED3\u679C",
  success: "\u6210\u529F",
  failed: "\u5931\u8D25",
  blockedTitle: "占用进程处理",
  blockedHint: "发现该文件被占用。可以直接查看占用进程并尝试结束，然后重试当前文件。",
  findBlockers: "查找占用进程",
  blockerEmpty: "当前没有查到明确的占用进程，可能进程已经退出或系统暂未返回结果。",
  terminateProcess: "结束进程",
  retrySingle: "重试当前文件",
  blockerLoading: "正在查找占用进程...",
  blockerProcess: "进程",
  blockerService: "服务",
  blockerRestartable: "可重启",
  blockerYes: "是",
  blockerNo: "否",
  suggestionsLimited:
    "\u5F53\u524D\u6E05\u7406\u9875\u4EC5\u5C55\u793A\u524D 1000 \u6761 AI \u5EFA\u8BAE\uFF0C\u5982\u679C\u662F\u5168\u76D8\u626B\u63CF\uFF0C\u8BF7\u4F18\u5148\u7F29\u5C0F\u8303\u56F4\u540E\u518D\u590D\u6838\u3002",
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
const message = useMessage();
const { executing, logs, error, executeCleanup, getPathBlockers } = useCleanup();
const { terminateProcess, terminating, terminateError } = useProcesses();

const report = computed(() => store.report);
const mode = ref<"recycle" | "move">("recycle");
const targetDir = ref("");
const dryRun = ref(true);
const showConfirm = ref(false);
const showResults = ref(false);
const showBlockers = ref(false);
const selectedPaths = ref<DataTableRowKey[]>([]);
const blockerLoading = ref(false);
const blockerPath = ref("");
const blockers = ref<BlockingProcessInfo[]>([]);
const retryEntry = ref<OperationLogEntry | null>(null);

const suggestions = computed(() =>
  (report.value?.advisor.suggestions ?? []).filter(
    (item) => item.action === "delete" || item.action === "move"
  )
);
const totalSuggestionCount = computed(
  () => report.value?.advisor.suggestionCount ?? report.value?.advisor.suggestions.length ?? 0
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
  if (mode.value === "move" && !targetDir.value.trim()) {
    message.error("请先选择目标目录");
    return;
  }
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

    // 如果不是模拟运行，从列表中移除已成功清理的文件
    if (!dryRun.value) {
      const successfulPaths = result.filter((entry) => entry.success).map((entry) => entry.path);
      if (successfulPaths.length > 0) {
        store.removeCleanedSuggestions(successfulPaths);
        // 同时清空选中的路径
        selectedPaths.value = selectedPaths.value.filter(
          (p) => !successfulPaths.includes(String(p))
        );
      }
    }
  }
}

function isOccupiedFailure(entry: OperationLogEntry) {
  return (
    !entry.success &&
    (entry.diagnosis?.code === "in_use_by_another_process" ||
      entry.diagnosis?.code === "locked_region")
  );
}

async function openBlockers(entry: OperationLogEntry) {
  blockerLoading.value = true;
  blockerPath.value = entry.path;
  retryEntry.value = entry;
  showBlockers.value = true;
  blockers.value = [];

  try {
    blockers.value = await getPathBlockers(entry.path);
  } finally {
    blockerLoading.value = false;
  }
}

async function handleTerminateBlocker(pid: number) {
  await terminateProcess(pid);
  if (!terminateError.value && blockerPath.value) {
    blockers.value = await getPathBlockers(blockerPath.value);
  }
}

async function pickTargetDirectory() {
  const selected = await open({ directory: true, multiple: false });
  if (selected && typeof selected === "string") {
    targetDir.value = selected;
  }
}

async function retryCurrentEntry() {
  if (!retryEntry.value) return;
  const result = await executeCleanup(
    [retryEntry.value.path],
    retryEntry.value.mode,
    retryEntry.value.mode === "move" ? targetDir.value || null : null,
    dryRun.value
  );
  if (result.length > 0) {
    store.addHistory(result);
    showBlockers.value = false;
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

  <div v-else class="page-shell cleanup-page">
    <section class="page-hero">
      <n-space vertical :size="18">
        <div>
          <div class="cleanup-hero-kicker">Action Desk</div>
          <div class="page-hero__title">清理执行中心</div>
          <div class="page-hero__desc">
            这里聚焦真正可执行的删除和移动动作。用户可以先筛选、先模拟，再正式执行；若遇到占用冲突，可以直接定位并结束相关进程。
          </div>
        </div>
        <div class="cleanup-top-grid">
          <div class="metric-card">
            <div class="metric-card__label">可执行建议</div>
            <div class="metric-card__value">{{ suggestions.length }}</div>
            <div class="metric-card__hint">当前支持直接进入清理流程的项目数量</div>
          </div>
          <div class="metric-card">
            <div class="metric-card__label">受保护项目</div>
            <div class="metric-card__value">{{ protectedSuggestionCount }}</div>
            <div class="metric-card__hint">保留或待审内容不会在这里直接清理</div>
          </div>
          <div class="metric-card">
            <div class="metric-card__label">已选项目</div>
            <div class="metric-card__value">{{ selectedPaths.length }}</div>
            <div class="metric-card__hint">执行前可继续增减选择范围</div>
          </div>
        </div>
      </n-space>
    </section>

    <n-card class="surface-card interactive-card">
      <template #header>
        <div class="section-head">
          <div>
            <div class="section-head__title">{{ TEXT.cleanupOptions }}</div>
            <div class="section-head__desc">先决定回收站还是移动目录，再决定是否仅做模拟运行。</div>
          </div>
        </div>
      </template>
      <div class="filter-bar cleanup-option-bar">
        <n-radio-group v-model:value="mode">
          <n-radio-button value="recycle">{{ TEXT.recycleBin }}</n-radio-button>
          <n-radio-button value="move">{{ TEXT.moveLabel }}</n-radio-button>
        </n-radio-group>
        <div v-if="mode === 'move'" class="cleanup-target-row">
          <n-input
            v-model:value="targetDir"
            :placeholder="TEXT.targetDir"
            class="cleanup-target-input"
          />
          <n-button secondary @click="pickTargetDirectory">
            {{ TEXT.browse }}
          </n-button>
        </div>
        <div class="cleanup-switch">
          <n-text>{{ TEXT.dryRun }}</n-text>
          <n-switch v-model:value="dryRun" />
        </div>
      </div>
    </n-card>

    <n-card class="surface-card interactive-card">
      <template #header>
        <div class="section-head">
          <div>
            <div class="section-head__title">{{ TEXT.chooseFiles }}</div>
            <div class="section-head__desc">
              只展示建议为删除或移动的内容，帮助用户聚焦真正会带来空间变化的文件。
            </div>
          </div>
        </div>
      </template>

      <n-space vertical :size="12">
        <n-alert v-if="report.advisor.truncated" type="warning">
          {{ TEXT.suggestionsLimited }}
        </n-alert>
        <n-alert v-if="protectedSuggestionCount > 0" type="info">
          {{ TEXT.protectedNotice }}
        </n-alert>
        <div class="soft-panel">
          <n-text depth="3">
            {{ totalSuggestionCount }} 条建议中，当前可执行列表展示 {{ suggestions.length }} 条。
          </n-text>
        </div>
        <n-data-table
          :columns="columns"
          :data="suggestions"
          :row-key="rowKey"
          :max-height="460"
          size="small"
          :bordered="false"
          v-model:checked-row-keys="selectedPaths"
        />
        <div class="cleanup-action-bar">
          <div class="cleanup-action-bar__summary">
            已选择 {{ selectedPaths.length }} 项，{{ dryRun ? "当前是模拟运行，不会改动文件。" : "当前是正式执行，会修改文件系统。" }}
          </div>
          <n-button
            type="primary"
            :disabled="selectedPaths.length === 0 || (mode.value === 'move' && !targetDir.value.trim()) || executing"
            :loading="executing"
            @click="handleConfirm"
          >
            {{ mode.value === 'move' ? TEXT.move : TEXT.executeCleanup }} ({{ TEXT.selected }} {{ selectedPaths.length }} {{ TEXT.item }})
          </n-button>
        </div>
      </n-space>
    </n-card>

    <n-alert v-if="error" type="error" :title="TEXT.error">
      {{ error }}
    </n-alert>

    <n-modal
      v-model:show="showConfirm"
      preset="dialog"
      :title="mode === 'move' ? TEXT.confirmMove : TEXT.confirmCleanup"
      :positive-text="dryRun ? TEXT.simulateRun : (mode === 'move' ? TEXT.confirmMoveAction : TEXT.confirmExecute)"
      :negative-text="TEXT.cancel"
      @positive-click="handleExecute"
      :type="dryRun ? 'info' : 'warning'"
    >
      <n-text>
        {{ dryRun ? TEXT.simulate : (mode === 'move' ? TEXT.move : TEXT.execute) }}
        {{ mode === 'recycle' ? TEXT.recycle : '' }}
        {{ selectedPaths.length }} {{ TEXT.fileQuestionSuffix }}
      </n-text>
      <n-alert v-if="!dryRun" type="warning" style="margin-top: 12px">
        {{ TEXT.realChangeWarning }}
      </n-alert>
    </n-modal>

    <n-modal v-model:show="showResults" preset="card" :title="mode === 'move' ? TEXT.moveResult : TEXT.executionResult" style="width: 600px">
      <n-space vertical :size="8">
        <div v-for="(entry, idx) in logs" :key="idx" class="cleanup-log-row">
          <n-tag :type="entry.success ? 'success' : 'error'" size="small" round>
            {{ entry.success ? TEXT.success : TEXT.failed }}
          </n-tag>
          <n-text class="cleanup-log-row__text">
            {{ entry.path.split(/[/\\]/).pop() }}: {{ entry.detail }}
          </n-text>
          <n-button
            v-if="isOccupiedFailure(entry)"
            size="tiny"
            secondary
            type="warning"
            @click="openBlockers(entry)"
          >
            {{ TEXT.findBlockers }}
          </n-button>
        </div>
      </n-space>
    </n-modal>

    <n-modal v-model:show="showBlockers" preset="card" :title="TEXT.blockedTitle" style="width: 720px">
      <n-space vertical :size="12">
        <n-alert type="warning">
          {{ TEXT.blockedHint }}
        </n-alert>
        <n-text style="word-break: break-all">{{ blockerPath }}</n-text>
        <n-text v-if="blockerLoading" depth="3">
          {{ TEXT.blockerLoading }}
        </n-text>
        <n-alert v-if="terminateError" type="error">
          {{ terminateError }}
        </n-alert>
        <n-empty
          v-if="!blockerLoading && blockers.length === 0"
          :description="TEXT.blockerEmpty"
        />
        <n-list v-else bordered>
          <n-list-item v-for="item in blockers" :key="item.pid">
            <n-space justify="space-between" align="center" style="width: 100%" :wrap="false">
              <div class="cleanup-blocker-card">
                <div style="font-weight: 600">{{ TEXT.blockerProcess }}：{{ item.appName }}</div>
                <n-text depth="3">PID：{{ item.pid }}</n-text>
                <n-text v-if="item.serviceName" depth="3" style="display: block">
                  {{ TEXT.blockerService }}：{{ item.serviceName }}
                </n-text>
                <n-text depth="3" style="display: block">
                  {{ TEXT.blockerRestartable }}：{{ item.restartable ? TEXT.blockerYes : TEXT.blockerNo }}
                </n-text>
              </div>
              <n-button
                type="error"
                secondary
                size="small"
                :loading="terminating"
                @click="handleTerminateBlocker(item.pid)"
              >
                {{ TEXT.terminateProcess }}
              </n-button>
            </n-space>
          </n-list-item>
        </n-list>
      </n-space>
      <template #footer>
        <n-space justify="end">
          <n-button @click="showBlockers = false">{{ TEXT.cancel }}</n-button>
          <n-button
            type="primary"
            :loading="executing"
            :disabled="!retryEntry"
            @click="retryCurrentEntry"
          >
            {{ TEXT.retrySingle }}
          </n-button>
        </n-space>
      </template>
    </n-modal>
  </div>
</template>

<style scoped>
.cleanup-page {
  gap: 18px;
}

.cleanup-hero-kicker {
  margin-bottom: 8px;
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.16em;
  text-transform: uppercase;
  color: var(--accent);
}

.cleanup-top-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
  gap: 14px;
}

.cleanup-option-bar {
  align-items: center;
}

.cleanup-target-row {
  display: flex;
  gap: 12px;
  align-items: center;
}

.cleanup-target-input {
  width: 320px;
}

.cleanup-switch {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-left: auto;
}

.cleanup-action-bar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  padding: 16px;
  border-radius: 18px;
  background: linear-gradient(135deg, #ffffff 0%, #f8fbff 100%);
  border: 1px solid var(--border-soft);
  box-shadow: inset 0 0 0 1px rgba(23, 133, 108, 0.06);
}

.cleanup-action-bar__summary {
  color: var(--text-normal);
  line-height: 1.7;
}

.cleanup-log-row {
  display: flex;
  align-items: center;
  gap: 10px;
}

.cleanup-log-row__text {
  font-size: 13px;
  line-height: 1.6;
}

.cleanup-blocker-card {
  min-width: 0;
}

@media (max-width: 900px) {
  .cleanup-switch {
    margin-left: 0;
  }

  .cleanup-target-row {
    width: 100%;
  }

  .cleanup-target-input {
    flex: 1;
  }

  .cleanup-action-bar {
    flex-direction: column;
    align-items: stretch;
  }
}
</style>
