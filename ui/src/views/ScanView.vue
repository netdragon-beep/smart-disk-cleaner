<script setup lang="ts">
import { ref } from "vue";
import { useRouter } from "vue-router";
import {
  NButton,
  NInput,
  NCard,
  NSpace,
  NProgress,
  NText,
  NAlert,
  NStatistic,
  NGrid,
  NGi,
} from "naive-ui";
import { open } from "@tauri-apps/plugin-dialog";
import { useScan } from "@/composables/useScan";
import { useAppStore } from "@/stores/app";
import type { ProgressEvent } from "@/types";

const TEXT = {
  scanDir: "\u626B\u63CF\u76EE\u5F55",
  pickPlaceholder: "\u8BF7\u9009\u62E9\u8981\u626B\u63CF\u7684\u76EE\u5F55...",
  browse: "\u6D4F\u89C8",
  scanning: "\u626B\u63CF\u4E2D...",
  startScan: "\u5F00\u59CB\u626B\u63CF",
  cancel: "\u53D6\u6D88",
  fileCount: "\u6587\u4EF6\u6570",
  dirCount: "\u76EE\u5F55\u6570",
  totalSize: "\u603B\u5927\u5C0F",
  scanError: "\u626B\u63CF\u9519\u8BEF",
  ready: "\u5C31\u7EEA",
  walkDone: "\u904D\u5386\u5B8C\u6210\uFF1A\u53D1\u73B0",
  filesUnit: "\u4E2A\u6587\u4EF6\uFF0C\u5171",
  walking: "\u6B63\u5728\u904D\u5386\uFF1A\u5DF2\u53D1\u73B0",
  dedupDone: "\u53BB\u91CD\u5B8C\u6210",
  hashing: "\u6B63\u5728\u54C8\u5E0C\u8BA1\u7B97\uFF1A",
  analyzing: "\u6B63\u5728\u5206\u6790\u6587\u4EF6\u7C7B\u578B\u548C\u5927\u5C0F...",
  advising: "\u6B63\u5728\u751F\u6210\u6E05\u7406\u5EFA\u8BAE...",
  scanDone: "\u626B\u63CF\u5B8C\u6210",
  processing: "\u6B63\u5728\u5904\u7406...",
};

const router = useRouter();
const store = useAppStore();
const { scanning, progress, error, startScan, cancelScan } = useScan();
const selectedPath = ref("");

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
  if (bytes < 1024) return bytes + " B";
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + " KB";
  if (bytes < 1024 * 1024 * 1024)
    return (bytes / (1024 * 1024)).toFixed(1) + " MB";
  return (bytes / (1024 * 1024 * 1024)).toFixed(2) + " GB";
}

function progressPercent(p: ProgressEvent | null): number {
  if (!p) return 0;
  if (p.kind === "Scan") {
    return p.phase === "done" ? 33 : Math.min(30, p.filesFound / 100);
  }
  if (p.kind === "Dedup") {
    if (p.filesTotal === 0) return 66;
    return 33 + (p.filesHashed / p.filesTotal) * 33;
  }
  if (p.kind === "Analyze") {
    if (p.phase === "done") return 100;
    if (p.phase === "advising") return 85;
    return 70;
  }
  return 0;
}

function progressLabel(p: ProgressEvent | null): string {
  if (!p) return TEXT.ready;
  if (p.kind === "Scan") {
    if (p.phase === "done") {
      return `${TEXT.walkDone} ${p.filesFound} ${TEXT.filesUnit} ${formatBytes(p.bytesFound)}`;
    }
    return `${TEXT.walking} ${p.filesFound} \u4E2A\u6587\u4EF6 (${formatBytes(p.bytesFound)})`;
  }
  if (p.kind === "Dedup") {
    if (p.phase === "done") return TEXT.dedupDone;
    return `${TEXT.hashing}${p.filesHashed} / ${p.filesTotal}`;
  }
  if (p.kind === "Analyze") {
    if (p.phase === "analyzing") return TEXT.analyzing;
    if (p.phase === "advising") return TEXT.advising;
    if (p.phase === "done") return TEXT.scanDone;
    return TEXT.processing;
  }
  return "";
}
</script>

<template>
  <div style="max-width: 700px; margin: 0 auto">
    <n-card :title="TEXT.scanDir">
      <n-space vertical :size="16">
        <n-space>
          <n-input
            v-model:value="selectedPath"
            :placeholder="TEXT.pickPlaceholder"
            style="width: 400px"
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
            :percentage="Math.round(progressPercent(progress))"
            :show-indicator="true"
            status="info"
          />
          <n-text depth="3" style="margin-top: 8px; display: block">
            {{ progressLabel(progress) }}
          </n-text>

          <n-grid :cols="3" :x-gap="12" style="margin-top: 12px" v-if="progress && progress.kind === 'Scan'">
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
