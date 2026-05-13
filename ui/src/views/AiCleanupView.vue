<script setup lang="ts">
import { computed, ref } from "vue";
import { useRouter } from "vue-router";
import { NAlert, NButton, NCard, NCheckbox, NEmpty, NSpace, NTag, NText } from "naive-ui";
import { useAiCleanup } from "@/composables/useAiCleanup";
import { useAppStore } from "@/stores/app";

const TEXT = {
  noPlan: "还没有 AI 一键整理结果，请先从扫描页发起 AI 一键整理。",
  goScan: "返回扫描页",
  title: "AI 一键整理建议",
  subtitle:
    "AI 会先总结更适合清理的文件，再结合扫描目录中的已安装软件给出可卸载候选。你来决定执行哪些项目。",
  fileSuggestions: "文件建议",
  uninstallCandidates: "软件卸载候选",
  delete: "建议删除",
  move: "建议移动",
  review: "建议复核",
  keep: "建议保留",
  riskLow: "低风险",
  riskMedium: "中风险",
  riskHigh: "高风险",
  openApps: "打开系统卸载列表",
  openAppsFailed: "打开系统卸载列表失败",
} as const;

const router = useRouter();
const store = useAppStore();
const { openAppsAndFeatures, error } = useAiCleanup();
const selectedFiles = ref<string[]>([]);
const selectedApps = ref<string[]>([]);
const plan = computed(() => store.aiCleanupPlan);

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function actionLabel(action: string) {
  if (action === "delete") return TEXT.delete;
  if (action === "move") return TEXT.move;
  if (action === "keep") return TEXT.keep;
  return TEXT.review;
}

function riskLabel(risk: string) {
  if (risk === "low") return TEXT.riskLow;
  if (risk === "medium") return TEXT.riskMedium;
  return TEXT.riskHigh;
}

function riskType(risk: string): "success" | "warning" | "error" {
  if (risk === "low") return "success";
  if (risk === "medium") return "warning";
  return "error";
}

async function handleOpenApps() {
  const ok = await openAppsAndFeatures();
  if (!ok) {
    window.alert(error.value || TEXT.openAppsFailed);
  }
}

function updateSelectedFiles(path: string, checked: boolean) {
  selectedFiles.value = checked
    ? [...selectedFiles.value, path]
    : selectedFiles.value.filter((item) => item !== path);
}

function updateSelectedApps(key: string, checked: boolean) {
  selectedApps.value = checked
    ? [...selectedApps.value, key]
    : selectedApps.value.filter((item) => item !== key);
}
</script>

<template>
  <div v-if="!plan" class="page-shell">
    <n-empty :description="TEXT.noPlan">
      <template #extra>
        <n-button @click="router.push({ name: 'scan' })">{{ TEXT.goScan }}</n-button>
      </template>
    </n-empty>
  </div>

  <div v-else class="page-shell">
    <section class="page-hero">
      <div class="page-hero__title">{{ TEXT.title }}</div>
      <div class="page-hero__desc">{{ TEXT.subtitle }}</div>
    </section>

    <n-card class="surface-card interactive-card">
      <template #header>
        <div class="section-head">
          <div>
            <div class="section-head__title">AI 总结</div>
            <div class="section-head__desc">先看总体结论，再决定要处理哪些文件和软件。</div>
          </div>
        </div>
      </template>
      <n-space vertical :size="12">
        <n-tag round>{{ plan.source }}</n-tag>
        <div class="soft-panel">
          <n-text style="display: block; white-space: pre-wrap">{{ plan.summary }}</n-text>
        </div>
      </n-space>
    </n-card>

    <n-card class="surface-card interactive-card">
      <template #header>
        <div class="section-head">
          <div>
            <div class="section-head__title">{{ TEXT.fileSuggestions }} ({{ plan.fileSuggestions.length }})</div>
            <div class="section-head__desc">先勾选你想处理的文件。真正执行删除/移动可以接到现有清理流程里。</div>
          </div>
        </div>
      </template>
      <n-space vertical :size="12">
        <div v-for="item in plan.fileSuggestions" :key="item.path" class="soft-panel">
          <n-space vertical :size="8">
            <n-checkbox
              :checked="selectedFiles.includes(item.path)"
              @update:checked="(checked) => updateSelectedFiles(item.path, checked)"
            >
              {{ item.path }}
            </n-checkbox>
            <n-space>
              <n-tag size="small" round>{{ actionLabel(item.action) }}</n-tag>
              <n-tag size="small" round :type="riskType(item.risk)">{{ riskLabel(item.risk) }}</n-tag>
            </n-space>
            <n-text depth="3">{{ item.reason }}</n-text>
          </n-space>
        </div>
      </n-space>
    </n-card>

    <n-card class="surface-card interactive-card">
      <template #header>
        <div class="section-head">
          <div>
            <div class="section-head__title">{{ TEXT.uninstallCandidates }} ({{ plan.uninstallCandidates.length }})</div>
            <div class="section-head__desc">这些软件在当前扫描目录下占用较大空间。如果你几乎不用它们，可以考虑卸载。</div>
          </div>
          <n-button secondary @click="handleOpenApps">{{ TEXT.openApps }}</n-button>
        </div>
      </template>
      <n-space vertical :size="12">
        <n-alert v-if="error" type="error">{{ error }}</n-alert>
        <div v-for="item in plan.uninstallCandidates" :key="item.key" class="soft-panel">
          <n-space vertical :size="8">
            <n-checkbox
              :checked="selectedApps.includes(item.key)"
              @update:checked="(checked) => updateSelectedApps(item.key, checked)"
            >
              {{ item.appName }}
            </n-checkbox>
            <n-space>
              <n-tag size="small" round>{{ item.publisher || "未知厂商" }}</n-tag>
              <n-tag size="small" round type="success">{{ formatBytes(item.estimatedSize) }}</n-tag>
              <n-tag size="small" round :type="item.uninstallAvailable ? 'info' : 'warning'">
                {{ item.uninstallAvailable ? "可直接卸载" : "需手动查找卸载入口" }}
              </n-tag>
            </n-space>
            <n-text depth="3">{{ item.reason }}</n-text>
            <n-text depth="3" v-if="item.installLocation">{{ item.installLocation }}</n-text>
          </n-space>
        </div>
      </n-space>
    </n-card>
  </div>
</template>
