<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import {
  NAlert,
  NButton,
  NCard,
  NGi,
  NGrid,
  NInput,
  NProgress,
  NSpace,
  NStatistic,
  NTag,
  NText,
} from "naive-ui";
import { open } from "@tauri-apps/plugin-dialog";
import { useScan } from "@/composables/useScan";
import { useAppStore } from "@/stores/app";
import type { ProgressEvent } from "@/types";

const TEXT = {
  scanDir: "扫描目录",
  pickPlaceholder: "请选择要扫描的目录...",
  browse: "浏览",
  scanning: "扫描中...",
  startScan: "开始扫描",
  cancel: "取消",
  fileCount: "文件数",
  dirCount: "目录数",
  totalSize: "总大小",
  scanError: "扫描错误",
  ready: "就绪",
  walkDone: "遍历完成：发现",
  filesUnit: "个文件，共",
  walking: "正在遍历目录",
  dedupDone: "去重完成",
  hashing: "正在计算重复文件指纹",
  analyzing: "正在分析文件类型和大小",
  advising: "正在生成清理建议",
  scanDone: "扫描完成",
  processing: "正在处理...",
  cached: "检测到磁盘无变化，正在直接复用上次扫描结果",
  starting: "正在启动扫描任务",
  stageWalking: "1. 遍历目录",
  stageDedup: "2. 重复文件检测",
  stageAnalyze: "3. 结果分析",
  currentPhase: "当前阶段",
  currentPath: "当前路径",
  elapsed: "已运行",
  noPathYet: "正在准备扫描上下文...",
  walkHint: "正在读取目录结构、统计文件数量和体积。",
  dedupHint: "正在对候选重复文件做指纹计算，这一步通常最耗时。",
  analyzeHint: "正在汇总扫描结果并生成清理建议。",
};

const router = useRouter();
const store = useAppStore();
const { scanning, progress, error, startScan, cancelScan } = useScan();
const selectedPath = ref("");
const elapsedSeconds = ref(0);
const displayedProgressPercent = ref(0);
let elapsedTimer: ReturnType<typeof setInterval> | null = null;
let progressTimer: ReturnType<typeof setInterval> | null = null;
type PhaseState = "wait" | "process" | "finish";

async function pickDirectory() {
  const selected = await open({ directory: true, multiple: false });
  if (selected && typeof selected === "string") {
    selectedPath.value = selected;
  }
}

async function handleScan() {
  if (!selectedPath.value) return;
  const result = await startScan(selectedPath.value);
  if (result) {
    store.setReport(result);
    router.push({ name: "results" });
  }
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function formatElapsed(seconds: number): string {
  if (seconds < 60) return `${seconds} 秒`;
  const minutes = Math.floor(seconds / 60);
  const remainSeconds = seconds % 60;
  return `${minutes} 分 ${remainSeconds} 秒`;
}

function progressPercent(p: ProgressEvent | null): number {
  if (!p) return 3;
  if (p.kind === "Scan") {
    return p.phase === "done" ? 35 : Math.max(6, Math.min(35, 6 + Math.log10(p.filesFound + 1) * 8));
  }
  if (p.kind === "Dedup") {
    if (p.phase === "done") return 72;
    if (p.filesTotal === 0) return 60;
    return 35 + (p.filesHashed / p.filesTotal) * 37;
  }
  if (p.kind === "Analyze") {
    if (p.phase === "cached") return 92;
    if (p.phase === "done") return 100;
    if (p.phase === "advising") return 88;
    return 76;
  }
  return 0;
}

function progressCap(p: ProgressEvent | null): number {
  if (!p) return 14;
  if (p.kind === "Scan") return p.phase === "done" ? 35 : 30;
  if (p.kind === "Dedup") return p.phase === "done" ? 72 : 68;
  if (p.phase === "cached") return 96;
  if (p.phase === "done") return 100;
  if (p.phase === "advising") return 94;
  return 86;
}

function progressStep(p: ProgressEvent | null): number {
  if (!p) return 1;
  if (p.kind === "Scan") return 1;
  if (p.kind === "Dedup") return 1;
  return 0.5;
}

function syncDisplayedProgress(force = false) {
  const target = Math.round(progressPercent(progress.value));
  if (force) {
    displayedProgressPercent.value = target;
    return;
  }
  if (target > displayedProgressPercent.value) {
    displayedProgressPercent.value = target;
  }
}

function progressLabel(p: ProgressEvent | null): string {
  if (!p) return TEXT.starting;
  if (p.kind === "Scan") {
    if (p.phase === "done") {
      return `${TEXT.walkDone} ${p.filesFound} ${TEXT.filesUnit} ${formatBytes(p.bytesFound)}`;
    }
    return `${TEXT.walking}：已发现 ${p.filesFound} 个文件，累计 ${formatBytes(p.bytesFound)}`;
  }
  if (p.kind === "Dedup") {
    if (p.phase === "done") return TEXT.dedupDone;
    return `${TEXT.hashing}：${p.filesHashed} / ${p.filesTotal}`;
  }
  if (p.kind === "Analyze") {
    if (p.phase === "cached") return TEXT.cached;
    if (p.phase === "analyzing") return TEXT.analyzing;
    if (p.phase === "advising") return TEXT.advising;
    if (p.phase === "done") return TEXT.scanDone;
    return p.detail || TEXT.processing;
  }
  return "";
}

const currentPhaseTitle = computed(() => {
  const p = progress.value;
  if (!p) return TEXT.starting;
  if (p.kind === "Scan") return TEXT.stageWalking;
  if (p.kind === "Dedup") return TEXT.stageDedup;
  return TEXT.stageAnalyze;
});

const currentPhaseHint = computed(() => {
  const p = progress.value;
  if (!p) return TEXT.walkHint;
  if (p.kind === "Scan") return TEXT.walkHint;
  if (p.kind === "Dedup") return TEXT.dedupHint;
  return TEXT.analyzeHint;
});

const currentPathText = computed(() => {
  const p = progress.value;
  if (!p) return selectedPath.value || TEXT.noPathYet;
  if (p.kind === "Analyze") return p.detail || TEXT.noPathYet;
  return p.currentPath || TEXT.noPathYet;
});

const phaseStates = computed(() => {
  const p = progress.value;
  const walking: PhaseState =
    !p || p.kind === "Scan"
      ? "process"
      : "finish";
  const dedup: PhaseState =
    !p || p.kind === "Scan"
      ? "wait"
      : p.kind === "Dedup"
        ? "process"
        : "finish";
  const analyze: PhaseState =
    p && p.kind === "Analyze"
      ? p.phase === "done"
        ? "finish"
        : "process"
      : "wait";

  return [
    { label: TEXT.stageWalking, state: walking },
    { label: TEXT.stageDedup, state: dedup },
    { label: TEXT.stageAnalyze, state: analyze },
  ];
});

function phaseTagType(
  state: PhaseState
): "default" | "info" | "success" {
  if (state === "finish") return "success";
  if (state === "process") return "info";
  return "default";
}

function phaseTagLabel(state: PhaseState): string {
  if (state === "finish") return "已完成";
  if (state === "process") return "进行中";
  return "等待中";
}

watch(
  scanning,
  (value) => {
    if (elapsedTimer) {
      clearInterval(elapsedTimer);
      elapsedTimer = null;
    }
    if (progressTimer) {
      clearInterval(progressTimer);
      progressTimer = null;
    }
    if (value) {
      elapsedSeconds.value = 0;
      displayedProgressPercent.value = Math.max(3, Math.round(progressPercent(progress.value)));
      elapsedTimer = setInterval(() => {
        elapsedSeconds.value += 1;
      }, 1000);
      progressTimer = setInterval(() => {
        syncDisplayedProgress();
        const cap = progressCap(progress.value);
        if (displayedProgressPercent.value < cap) {
          displayedProgressPercent.value = Math.min(
            cap,
            Number((displayedProgressPercent.value + progressStep(progress.value)).toFixed(1))
          );
        }
      }, 900);
    } else if (displayedProgressPercent.value < 100) {
      displayedProgressPercent.value = 0;
    }
  },
  { immediate: true }
);

watch(
  progress,
  () => {
    syncDisplayedProgress();
    if (progress.value?.kind === "Analyze" && progress.value.phase === "done") {
      displayedProgressPercent.value = 100;
    }
  },
  { deep: true }
);

onUnmounted(() => {
  if (elapsedTimer) {
    clearInterval(elapsedTimer);
  }
  if (progressTimer) {
    clearInterval(progressTimer);
  }
});
</script>

<template>
  <div style="max-width: 760px; margin: 0 auto">
    <n-card :title="TEXT.scanDir">
      <n-space vertical :size="16">
        <n-space>
          <n-input
            v-model:value="selectedPath"
            :placeholder="TEXT.pickPlaceholder"
            style="width: 460px"
            :disabled="scanning"
          />
          <n-button @click="pickDirectory" :disabled="scanning">
            {{ TEXT.browse }}
          </n-button>
        </n-space>

        <n-space>
          <n-button
            type="primary"
            @click="handleScan"
            :disabled="!selectedPath || scanning"
            :loading="scanning"
          >
            {{ scanning ? TEXT.scanning : TEXT.startScan }}
          </n-button>
          <n-button v-if="scanning" type="error" @click="cancelScan">
            {{ TEXT.cancel }}
          </n-button>
        </n-space>

        <div v-if="scanning">
          <n-progress
            type="line"
            :percentage="Math.round(displayedProgressPercent)"
            :show-indicator="true"
            status="info"
            processing
            :height="18"
            :border-radius="999"
            indicator-placement="inside"
          />

          <n-card size="small" embedded style="margin-top: 14px">
            <n-space vertical :size="12">
              <n-space justify="space-between" align="center">
                <div>
                  <n-text depth="3">{{ TEXT.currentPhase }}</n-text>
                  <n-text style="display: block; margin-top: 4px; font-size: 16px; font-weight: 600">
                    {{ currentPhaseTitle }}
                  </n-text>
                </div>
                <n-tag type="info" size="small">
                  {{ TEXT.elapsed }}：{{ formatElapsed(elapsedSeconds) }}
                </n-tag>
              </n-space>

              <n-text style="display: block; white-space: pre-wrap">
                {{ progressLabel(progress) }}
              </n-text>

              <n-text depth="3" style="display: block; white-space: pre-wrap">
                {{ currentPhaseHint }}
              </n-text>

              <div>
                <n-text depth="3">{{ TEXT.currentPath }}</n-text>
                <div
                  style="
                    margin-top: 6px;
                    min-height: 56px;
                    max-height: 56px;
                    overflow: auto;
                    padding-right: 4px;
                  "
                >
                  <n-text style="display: block; word-break: break-all">
                    {{ currentPathText }}
                  </n-text>
                </div>
              </div>
            </n-space>
          </n-card>

          <n-space style="margin-top: 12px" wrap>
            <n-tag
              v-for="item in phaseStates"
              :key="item.label"
              :type="phaseTagType(item.state)"
              round
              size="medium"
            >
              {{ item.label }} · {{ phaseTagLabel(item.state) }}
            </n-tag>
          </n-space>

          <n-grid
            v-if="progress && progress.kind === 'Scan'"
            :cols="3"
            :x-gap="12"
            style="margin-top: 14px"
          >
            <n-gi>
              <n-statistic :label="TEXT.fileCount" :value="progress.filesFound" />
            </n-gi>
            <n-gi>
              <n-statistic :label="TEXT.dirCount" :value="progress.dirsVisited" />
            </n-gi>
            <n-gi>
              <n-statistic :label="TEXT.totalSize" :value="formatBytes(progress.bytesFound)" />
            </n-gi>
          </n-grid>
        </div>

        <n-alert v-if="error" type="error" :title="TEXT.scanError">
          {{ error }}
        </n-alert>
      </n-space>
    </n-card>
  </div>
</template>
