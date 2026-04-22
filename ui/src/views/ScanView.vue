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

type PhaseState = "wait" | "process" | "finish";

const TEXT = {
  title: "开始一次新的磁盘扫描",
  subtitle:
    "先选择目录，再让系统自动完成遍历、去重和清理建议分析。整个过程会把阶段、路径和进度持续展示出来，避免用户不知道现在扫到了哪里。",
  pathLabel: "扫描目录",
  pathPlaceholder: "请选择要扫描的磁盘或目录，例如 C:/Users/你的用户名",
  browse: "浏览目录",
  startScan: "开始扫描",
  scanning: "扫描中",
  cancel: "取消任务",
  currentPhase: "当前阶段",
  currentPath: "当前路径",
  elapsed: "已运行",
  noPathYet: "正在准备扫描任务...",
  fileCount: "已发现文件",
  dirCount: "已遍历目录",
  totalSize: "累计体积",
  scanError: "扫描失败",
  walkDone: "目录遍历完成",
  walking: "正在遍历目录",
  dedupDone: "重复文件识别完成",
  hashing: "正在识别重复文件",
  analyzing: "正在分析文件类型和大小",
  advising: "正在生成清理建议",
  scanDone: "扫描完成",
  processing: "正在处理",
  cached: "检测到目录变化较小，正在复用上次扫描结果",
  starting: "正在启动扫描任务",
  stageWalking: "1. 目录遍历",
  stageDedup: "2. 重复文件识别",
  stageAnalyze: "3. 分析与建议",
  walkHint: "先读取目录结构，统计文件数量和体积，建立这次扫描的基础视图。",
  dedupHint: "对候选文件计算指纹，确认哪些是真正的重复内容。这一步通常最耗时。",
  analyzeHint: "汇总结果并生成可执行建议，供后续清理和迁移页面直接使用。",
  ready: "待开始",
  running: "进行中",
  finished: "已完成",
  tipsTitle: "这次会得到什么",
  tip1: "自动识别大文件、临时文件、压缩包和重复文件。",
  tip2: "结果页支持按应用卡片、文件类型和关键词联动筛选。",
  tip3: "后续清理和迁移都可以继续复用这次扫描结果。",
};

const router = useRouter();
const store = useAppStore();
const { scanning, progress, error, startScan, cancelScan } = useScan();
const selectedPath = ref("");
const elapsedSeconds = ref(0);
const displayedProgressPercent = ref(0);
const lastKnownPath = ref("");
let elapsedTimer: ReturnType<typeof setInterval> | null = null;
let progressTimer: ReturnType<typeof setInterval> | null = null;

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
    displayedProgressPercent.value = 100;
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

function progressPercent(event: ProgressEvent | null): number {
  if (!event) return 3;
  if (event.kind === "Scan") {
    return event.phase === "done"
      ? 35
      : Math.max(6, Math.min(35, 6 + Math.log10(event.filesFound + 1) * 8));
  }
  if (event.kind === "Dedup") {
    if (event.phase === "done") return 72;
    if (event.filesTotal === 0) return 60;
    return 35 + (event.filesHashed / event.filesTotal) * 37;
  }
  if (event.phase === "cached") return 92;
  if (event.phase === "hash_cache") return 74;
  if (event.phase === "type_breakdown") return 80;
  if (event.phase === "done") return 99;
  if (event.phase === "advising") return 88;
  if (event.phase === "caching_report") return 97;
  if (event.phase === "packaging") return 95;
  return 76;
}

function progressCap(event: ProgressEvent | null): number {
  if (!event) return 14;
  if (event.kind === "Scan") return event.phase === "done" ? 35 : 30;
  if (event.kind === "Dedup") return event.phase === "done" ? 72 : 68;
  if (event.phase === "cached") return 96;
  if (event.phase === "hash_cache") return 76;
  if (event.phase === "type_breakdown") return 84;
  if (event.phase === "done") return 99;
  if (event.phase === "advising") return 94;
  if (event.phase === "caching_report") return 98;
  if (event.phase === "packaging") return 98;
  return 86;
}

function progressStep(event: ProgressEvent | null): number {
  if (!event) return 1;
  if (event.kind === "Analyze") return 0.5;
  return 1;
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

function progressLabel(event: ProgressEvent | null): string {
  if (!event) return TEXT.starting;
  if (event.kind === "Scan") {
    if (event.phase === "done") {
      return `${TEXT.walkDone}，共发现 ${event.filesFound} 个文件，总计 ${formatBytes(event.bytesFound)}`;
    }
    return `${TEXT.walking}，已发现 ${event.filesFound} 个文件，累计 ${formatBytes(event.bytesFound)}`;
  }
  if (event.kind === "Dedup") {
    if (event.phase === "done") return TEXT.dedupDone;
    return `${TEXT.hashing}，${event.filesHashed} / ${event.filesTotal}`;
  }
  if (event.phase === "cached") return TEXT.cached;
  if (event.phase === "hash_cache") return "正在准备重复文件哈希缓存...";
  if (event.phase === "type_breakdown") return event.detail || "正在统计文件类型分布...";
  if (event.phase === "analyzing") return TEXT.analyzing;
  if (event.phase === "advising") return TEXT.advising;
  if (event.phase === "caching_report") return "正在写入扫描缓存并整理最终结果...";
  if (event.phase === "packaging") return event.detail || "正在整理结果给界面展示...";
  if (event.phase === "done") return TEXT.scanDone;
  return event.detail || TEXT.processing;
}

const currentPhaseTitle = computed(() => {
  const event = progress.value;
  if (!event) return TEXT.starting;
  if (event.kind === "Scan") return TEXT.stageWalking;
  if (event.kind === "Dedup") return TEXT.stageDedup;
  if (event.phase === "hash_cache") return "2. 准备哈希缓存";
  if (event.phase === "type_breakdown") return "3. 统计类型分布";
  if (event.phase === "advising") return "3. 生成清理建议";
  if (event.phase === "packaging") return "3. 整理结果";
  return TEXT.stageAnalyze;
});

const currentPhaseHint = computed(() => {
  const event = progress.value;
  if (!event) return TEXT.walkHint;
  if (event.kind === "Scan") return TEXT.walkHint;
  if (event.kind === "Dedup") return TEXT.dedupHint;
  if (event.phase === "hash_cache") {
    return "正在加载和写入重复文件哈希缓存，这一步属于重复文件识别后的收尾准备。";
  }
  if (event.phase === "type_breakdown") {
    return "正在统计文件类型分布、体积和分类结果，这一步会遍历整批扫描文件。";
  }
  if (event.phase === "advising") {
    return "正在根据分析结果和重复文件结果生成本地清理建议。";
  }
  if (event.phase === "packaging") {
    return "正在整理前端展示结果，并汇总这次扫描各个模块的数据。";
  }
  return TEXT.analyzeHint;
});

const currentPathText = computed(() => {
  const event = progress.value;
  if (!event) return lastKnownPath.value || selectedPath.value || TEXT.noPathYet;
  if (event.kind === "Analyze") {
    return lastKnownPath.value || selectedPath.value || TEXT.noPathYet;
  }
  return event.currentPath || lastKnownPath.value || selectedPath.value || TEXT.noPathYet;
});

const phaseStates = computed(() => {
  const event = progress.value;
  const walking: PhaseState = !event || event.kind === "Scan" ? "process" : "finish";
  const dedup: PhaseState =
    !event || event.kind === "Scan"
      ? "wait"
      : event.kind === "Dedup" || (event.kind === "Analyze" && event.phase === "hash_cache")
        ? "process"
        : "finish";
  const analyze: PhaseState =
    event && event.kind === "Analyze"
      ? event.phase === "done"
        ? "finish"
        : event.phase === "hash_cache"
          ? "wait"
          : "process"
      : "wait";

  return [
    { label: TEXT.stageWalking, state: walking },
    { label: TEXT.stageDedup, state: dedup },
    { label: TEXT.stageAnalyze, state: analyze },
  ];
});

const tips = [TEXT.tip1, TEXT.tip2, TEXT.tip3];

function phaseTagType(state: PhaseState): "default" | "info" | "success" {
  if (state === "finish") return "success";
  if (state === "process") return "info";
  return "default";
}

function phaseTagLabel(state: PhaseState): string {
  if (state === "finish") return TEXT.finished;
  if (state === "process") return TEXT.running;
  return TEXT.ready;
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
  (value) => {
    if (value?.kind === "Scan" && value.currentPath) {
      lastKnownPath.value = value.currentPath;
    }
    if (value?.kind === "Dedup" && value.currentPath) {
      lastKnownPath.value = value.currentPath;
    }
    syncDisplayedProgress();
  },
  { deep: true }
);

watch(
  scanning,
  (value) => {
    if (value) {
      lastKnownPath.value = selectedPath.value;
    }
  },
  { immediate: true }
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
  <div class="page-shell scan-page">
    <section class="page-hero">
      <n-space vertical :size="18">
        <div>
          <div class="scan-hero-kicker">Workspace Scan</div>
          <div class="page-hero__title">{{ TEXT.title }}</div>
          <div class="page-hero__desc">{{ TEXT.subtitle }}</div>
        </div>

        <div class="scan-hero-actions">
          <div class="scan-path-panel">
            <div class="scan-path-label">{{ TEXT.pathLabel }}</div>
            <div class="scan-path-row">
              <n-input
                v-model:value="selectedPath"
                :placeholder="TEXT.pathPlaceholder"
                :disabled="scanning"
                class="scan-input"
              />
              <n-button secondary @click="pickDirectory" :disabled="scanning">
                {{ TEXT.browse }}
              </n-button>
              <n-button
                type="primary"
                @click="handleScan"
                :disabled="!selectedPath || scanning"
                :loading="scanning"
              >
                {{ scanning ? TEXT.scanning : TEXT.startScan }}
              </n-button>
            </div>
          </div>

          <div class="scan-tips-card">
            <div class="scan-tips-title">{{ TEXT.tipsTitle }}</div>
            <ul class="scan-tips-list">
              <li v-for="tip in tips" :key="tip">{{ tip }}</li>
            </ul>
            <div class="scan-tips-footnote">让用户在开始前就知道扫描之后会发生什么。</div>
          </div>
        </div>
      </n-space>
    </section>

    <section v-if="scanning" class="metric-grid">
      <div class="metric-card">
        <div class="metric-card__label">{{ TEXT.elapsed }}</div>
        <div class="metric-card__value">{{ formatElapsed(elapsedSeconds) }}</div>
        <div class="metric-card__hint">实时展示本次扫描持续时长</div>
      </div>
      <div class="metric-card">
        <div class="metric-card__label">当前进度</div>
        <div class="metric-card__value">{{ Math.round(displayedProgressPercent) }}%</div>
        <div class="metric-card__hint">{{ currentPhaseTitle }}</div>
      </div>
      <div class="metric-card" v-if="progress?.kind === 'Scan'">
        <div class="metric-card__label">{{ TEXT.fileCount }}</div>
        <div class="metric-card__value">{{ progress.filesFound }}</div>
        <div class="metric-card__hint">扫描阶段实时统计</div>
      </div>
      <div class="metric-card" v-if="progress?.kind === 'Scan'">
        <div class="metric-card__label">{{ TEXT.totalSize }}</div>
        <div class="metric-card__value">{{ formatBytes(progress.bytesFound) }}</div>
        <div class="metric-card__hint">当前已发现内容体积</div>
      </div>
    </section>

    <n-card class="surface-card interactive-card">
      <template #header>
        <div class="section-head">
          <div>
            <div class="section-head__title">扫描状态</div>
            <div class="section-head__desc">
              三个阶段按顺序推进。用户可以看到系统当前在做什么，而不是只看到一个转圈。
            </div>
          </div>
          <n-button v-if="scanning" type="error" secondary @click="cancelScan">
            {{ TEXT.cancel }}
          </n-button>
        </div>
      </template>

      <n-space vertical :size="16">
        <n-progress
          type="line"
          :percentage="Math.round(displayedProgressPercent)"
          :show-indicator="true"
          status="info"
          :processing="scanning"
          :height="20"
          :border-radius="999"
          indicator-placement="inside"
        />

        <div class="scan-phase-list">
          <div
            v-for="item in phaseStates"
            :key="item.label"
            class="scan-phase-item"
            :class="`scan-phase-item--${item.state}`"
          >
            <div class="scan-phase-item__title">{{ item.label }}</div>
            <n-tag :type="phaseTagType(item.state)" round size="small">
              {{ phaseTagLabel(item.state) }}
            </n-tag>
          </div>
        </div>

        <div class="soft-panel">
          <n-space vertical :size="12">
            <div>
              <n-text depth="3">{{ TEXT.currentPhase }}</n-text>
              <div class="scan-current-title">{{ currentPhaseTitle }}</div>
            </div>
            <n-text style="display: block; white-space: pre-wrap">{{ progressLabel(progress) }}</n-text>
            <n-text depth="3" style="display: block; white-space: pre-wrap">
              {{ currentPhaseHint }}
            </n-text>
          </n-space>
        </div>

        <div class="soft-panel">
          <n-text depth="3">{{ TEXT.currentPath }}</n-text>
          <div class="scan-current-path scroll-area">
            {{ currentPathText }}
          </div>
        </div>

        <n-grid
          v-if="progress && progress.kind === 'Scan'"
          cols="1 s:3"
          responsive="screen"
          :x-gap="12"
          :y-gap="12"
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

        <n-alert v-if="error" type="error" :title="TEXT.scanError">
          {{ error }}
        </n-alert>
      </n-space>
    </n-card>
  </div>
</template>

<style scoped>
.scan-page {
  max-width: 1200px;
}

.scan-hero-kicker {
  margin-bottom: 8px;
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.16em;
  text-transform: uppercase;
  color: var(--accent);
}

.scan-hero-actions {
  display: grid;
  grid-template-columns: minmax(0, 1.55fr) minmax(280px, 0.9fr);
  gap: 16px;
}

.scan-path-panel,
.scan-tips-card {
  position: relative;
  padding: 18px;
  border-radius: 20px;
  background: rgba(255, 255, 255, 0.82);
  border: 1px solid rgba(230, 235, 243, 0.92);
  backdrop-filter: blur(10px);
  overflow: hidden;
}

.scan-path-panel::after,
.scan-tips-card::after {
  content: "";
  position: absolute;
  inset: auto -35px -35px auto;
  width: 110px;
  height: 110px;
  border-radius: 999px;
  background: radial-gradient(circle, rgba(23, 133, 108, 0.08), rgba(23, 133, 108, 0));
  pointer-events: none;
}

.scan-path-label,
.scan-tips-title {
  margin-bottom: 10px;
  font-size: 14px;
  font-weight: 700;
  color: var(--text-strong);
}

.scan-path-row {
  display: flex;
  gap: 12px;
  align-items: center;
}

.scan-input {
  flex: 1;
}

.scan-tips-list {
  margin: 0;
  padding-left: 18px;
  color: var(--text-normal);
  line-height: 1.8;
}

.scan-tips-footnote {
  margin-top: 12px;
  padding-top: 12px;
  border-top: 1px dashed var(--border-soft);
  font-size: 12px;
  color: var(--text-soft);
}

.scan-phase-list {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 12px;
}

.scan-phase-item {
  position: relative;
  padding: 16px;
  border-radius: 18px;
  border: 1px solid var(--border-soft);
  background: #fff;
  overflow: hidden;
}

.scan-phase-item::before {
  content: "";
  position: absolute;
  left: 16px;
  right: 16px;
  top: 0;
  height: 2px;
  background: linear-gradient(90deg, rgba(23, 133, 108, 0.75), rgba(59, 130, 246, 0.22));
}

.scan-phase-item--process {
  border-color: rgba(59, 130, 246, 0.28);
  background: linear-gradient(135deg, #ffffff 0%, #f5f9ff 100%);
}

.scan-phase-item--finish {
  border-color: rgba(31, 157, 115, 0.24);
  background: linear-gradient(135deg, #ffffff 0%, #f2fbf7 100%);
}

.scan-phase-item__title {
  margin-bottom: 10px;
  font-weight: 700;
  color: var(--text-strong);
}

.scan-current-title {
  margin-top: 6px;
  font-size: 20px;
  font-weight: 800;
  font-family:
    "Georgia",
    "Songti SC",
    serif;
  color: var(--text-strong);
}

.scan-current-path {
  margin-top: 8px;
  max-height: 88px;
  overflow: auto;
  line-height: 1.7;
  word-break: break-all;
  color: var(--text-normal);
}

@media (max-width: 900px) {
  .scan-hero-actions {
    grid-template-columns: 1fr;
  }

  .scan-path-row {
    flex-wrap: wrap;
  }

  .scan-phase-list {
    grid-template-columns: 1fr;
  }
}
</style>
