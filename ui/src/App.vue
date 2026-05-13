<script setup lang="ts">
import { computed, h, onErrorCaptured, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import {
  NAlert,
  NButton,
  NConfigProvider,
  NLayout,
  NLayoutContent,
  NLayoutSider,
  NMenu,
  NMessageProvider,
  NSpace,
  NSwitch,
  NTag,
  NText,
  NTooltip,
  darkTheme,
  dateZhCN,
  type GlobalThemeOverrides,
  type MenuOption,
  zhCN,
} from "naive-ui";
import { useConfig } from "@/composables/useConfig";
import { useAppStore } from "@/stores/app";
import type { AppConfig } from "@/types";

const router = useRouter();
const route = useRoute();
const store = useAppStore();
const { loadConfig, saveConfig } = useConfig();

const appTitle = "本地存储治理助手";
const renderEpoch = ref(0);
const routeError = ref("");
const themeSwitchLoading = ref(false);

const currentRouteName = computed(() => (route.name as string) || "scan");
const activeKey = computed(() => currentRouteName.value);
const viewKey = computed(() => `${route.fullPath}:${renderEpoch.value}`);
const isDark = computed(() => store.theme === "dark");

const pageMeta: Record<string, { title: string; subtitle: string; badge: string }> = {
  scan: {
    title: "空间发现",
    subtitle: "从目录扫描开始识别大文件、应用数据和典型占用场景。",
    badge: "Discover",
  },
  results: {
    title: "风险解释",
    subtitle: "按治理建议、应用概览和目录结构理解空间问题，而不是直接删除文件。",
    badge: "Explain",
  },
  cleanup: {
    title: "安全执行",
    subtitle: "将治理建议转成 dry-run 或真实操作，保留执行结果和占用诊断。",
    badge: "Act",
  },
  migration: {
    title: "安全迁移",
    subtitle: "围绕应用数据、大文件和下载归档生成结构化迁移计划与回滚链路。",
    badge: "Migrate",
  },
  registry: {
    title: "注册表治理",
    subtitle: "只处理可解释、可备份、可预览、可回滚的启动项与路径引用问题。",
    badge: "Registry",
  },
  processes: {
    title: "占用诊断",
    subtitle: "查看阻塞治理动作的高占用进程，并给出风险提示与处理建议。",
    badge: "Process",
  },
  history: {
    title: "治理历史",
    subtitle: "统一复核文件治理、迁移和注册表修改记录，保留回滚入口。",
    badge: "History",
  },
  settings: {
    title: "偏好设置",
    subtitle: "管理阈值、排除规则和 AI 配置，保持治理策略一致。",
    badge: "Config",
  },
  "ai-cleanup": {
    title: "AI 治理摘要",
    subtitle: "用 AI 辅助解释候选治理对象，但不绕过本地规则和人工确认。",
    badge: "AI",
  },
};

const currentPage = computed(() => pageMeta[currentRouteName.value] ?? pageMeta.scan);

const lightThemeOverrides: GlobalThemeOverrides = {
  common: {
    primaryColor: "#17856c",
    primaryColorHover: "#22957a",
    primaryColorPressed: "#116b56",
    primaryColorSuppl: "#1b8d73",
    infoColor: "#3b82f6",
    successColor: "#1f9d73",
    warningColor: "#f59e0b",
    errorColor: "#ef4444",
    borderRadius: "18px",
    borderColor: "#e5eaf3",
    textColorBase: "#182235",
    textColor1: "#182235",
    textColor2: "#516079",
    textColor3: "#74829a",
    bodyColor: "#f4f7fb",
    cardColor: "#ffffff",
    tableColor: "#ffffff",
    modalColor: "#ffffff",
    popoverColor: "#ffffff",
  },
  Layout: {
    siderColor: "#ffffff",
    color: "#f4f7fb",
    contentColor: "#f4f7fb",
    headerColor: "#f4f7fb",
    siderBorderColor: "#edf1f7",
  },
};

const darkThemeOverrides: GlobalThemeOverrides = {
  common: {
    primaryColor: "#22c55e",
    primaryColorHover: "#4ade80",
    primaryColorPressed: "#16a34a",
    primaryColorSuppl: "#22c55e",
    infoColor: "#3b82f6",
    successColor: "#22c55e",
    warningColor: "#f59e0b",
    errorColor: "#ef4444",
    borderRadius: "18px",
    borderColor: "#273549",
    textColorBase: "#e2e8f0",
    textColor1: "#f1f5f9",
    textColor2: "#cbd5e1",
    textColor3: "#94a3b8",
    bodyColor: "#0f172a",
    cardColor: "#1e293b",
    tableColor: "#1e293b",
    modalColor: "#1e293b",
    popoverColor: "#1e293b",
  },
  Layout: {
    siderColor: "#1e293b",
    color: "#0f172a",
    contentColor: "#0f172a",
    headerColor: "#0f172a",
    siderBorderColor: "#334155",
  },
};

const themeOverrides = computed(() =>
  isDark.value ? darkThemeOverrides : lightThemeOverrides
);

function buildMenuOption(label: string, key: string, description: string): MenuOption {
  return {
    key,
    label: () =>
      h(
        NTooltip,
        {
          delay: 300,
          placement: "right",
          style: "max-width: 260px;",
        },
        {
          trigger: () =>
            h(
              "div",
              {
                style:
                  "display:flex;align-items:center;min-width:0;width:100%;font-weight:600;",
              },
              label
            ),
          default: () => description,
        }
      ),
  };
}

const menuOptions: MenuOption[] = [
  buildMenuOption("空间发现", "scan", "选择磁盘或目录，启动空间扫描与场景识别。"),
  buildMenuOption("风险解释", "results", "用统一治理建议模型解释扫描结果和处理边界。"),
  buildMenuOption("安全执行", "cleanup", "将建议转成 dry-run 或真实执行，并保留诊断信息。"),
  buildMenuOption("安全迁移", "migration", "为应用数据和大文件生成结构化迁移计划。"),
  buildMenuOption("注册表治理", "registry", "只读分析、备份、预览、单次修改与回滚。"),
  buildMenuOption("占用诊断", "processes", "查看阻塞文件治理的进程并提供风险说明。"),
  buildMenuOption("治理历史", "history", "统一查看文件治理、迁移和注册表操作历史。"),
  buildMenuOption("偏好设置", "settings", "维护阈值、排除规则和 AI 配置。"),
];

function handleMenuUpdate(key: string) {
  router.push({ name: key });
}

function reloadCurrentView() {
  routeError.value = "";
  renderEpoch.value += 1;
}

async function handleThemeToggle(value: boolean) {
  const nextTheme = value ? "dark" : "light";
  const nextConfig: AppConfig = {
    largeFileThresholdMb: store.config?.largeFileThresholdMb ?? 512,
    maxAiItems: store.config?.maxAiItems ?? 20,
    apiKey: store.config?.apiKey ?? null,
    aiBaseUrl: store.config?.aiBaseUrl ?? "https://api.openai.com",
    aiModel: store.config?.aiModel ?? "gpt-4.1-mini",
    strictFileAiRemoteOnly: store.config?.strictFileAiRemoteOnly ?? false,
    excludePatterns: store.config?.excludePatterns ?? [],
    theme: nextTheme,
  };

  store.setConfig(nextConfig);
  themeSwitchLoading.value = true;
  await saveConfig(nextConfig);
  themeSwitchLoading.value = false;
}

watch(
  () => route.fullPath,
  () => {
    routeError.value = "";
  }
);

onErrorCaptured((error) => {
  routeError.value = error instanceof Error ? error.message : String(error ?? "未知前端错误");
  return false;
});

watch(
  () => store.theme,
  (newTheme) => {
    if (newTheme === "dark") {
      document.documentElement.classList.add("dark");
    } else {
      document.documentElement.classList.remove("dark");
    }
  },
  { immediate: true }
);

onMounted(async () => {
  const cfg = await loadConfig();
  if (cfg) {
    store.setConfig(cfg);
  }
});
</script>

<template>
  <n-config-provider
    :theme="isDark ? darkTheme : undefined"
    :theme-overrides="themeOverrides"
    :locale="zhCN"
    :date-locale="dateZhCN"
  >
    <n-message-provider>
      <n-layout has-sider class="app-shell">
        <n-layout-sider
          bordered
          class="app-sider"
          :width="250"
          :collapsed-width="84"
          show-trigger="bar"
          collapse-mode="width"
        >
          <div class="brand-panel">
            <div class="brand-mark">SD</div>
            <div class="brand-copy">
              <div class="brand-title">{{ appTitle }}</div>
              <n-text depth="3" class="brand-subtitle">
                面向 Windows 重度用户的可解释存储治理台
              </n-text>
            </div>
          </div>

          <div class="sider-note">
            <n-tag size="small" type="success" round>
              {{ isDark ? "深色主题" : "浅色主题" }}
            </n-tag>
            <n-text depth="3">
              以空间发现、风险解释、安全执行三条主线组织功能，避免把产品做成模糊的系统优化工具。
            </n-text>
          </div>

          <n-menu
            class="app-menu"
            :value="activeKey"
            :options="menuOptions"
            @update:value="handleMenuUpdate"
          />
        </n-layout-sider>

        <n-layout class="app-main">
          <n-layout-content content-style="height: 100%;">
            <div class="app-topbar">
              <div>
                <div class="topbar-kicker">Storage Governance Studio</div>
                <div class="topbar-title">{{ currentPage.title }}</div>
                <n-text depth="3">{{ currentPage.subtitle }}</n-text>
              </div>
              <div class="topbar-meta">
                <div class="theme-toggle-card">
                  <div class="theme-toggle-card__copy">
                    <div class="theme-toggle-card__label">主题切换</div>
                    <div class="theme-toggle-card__value">
                      {{ isDark ? "深色模式" : "浅色模式" }}
                    </div>
                  </div>
                  <n-switch
                    :value="isDark"
                    :loading="themeSwitchLoading"
                    @update:value="handleThemeToggle"
                  />
                </div>
                <n-tag round size="medium" type="success">
                  {{ isDark ? "Dark Editorial UI" : "Light Editorial UI" }}
                </n-tag>
                <n-tag round size="medium" type="info">{{ currentPage.badge }}</n-tag>
              </div>
            </div>

            <div class="view-scroll">
              <n-alert v-if="routeError" type="error" title="页面渲染失败" class="page-shell">
                <n-space vertical :size="12">
                  <div>{{ routeError }}</div>
                  <n-button @click="reloadCurrentView" style="width: 180px">
                    重新加载当前页面
                  </n-button>
                </n-space>
              </n-alert>
              <router-view v-else :key="viewKey" />
            </div>
          </n-layout-content>
        </n-layout>
      </n-layout>
    </n-message-provider>
  </n-config-provider>
</template>

<style>
:root {
  color-scheme: light;
  --app-bg: #f4f7fb;
  --surface: #ffffff;
  --surface-soft: #f7f9fc;
  --border-soft: #e6ebf3;
  --text-strong: #142033;
  --text-normal: #516079;
  --text-soft: #7b879b;
  --accent: #17856c;
  --shadow-soft: 0 18px 42px rgba(15, 23, 42, 0.06);
  --shadow-hover: 0 24px 52px rgba(15, 23, 42, 0.1);
  --radius-xl: 24px;
  --radius-lg: 20px;
}

:root.dark {
  color-scheme: dark;
  --app-bg: #0f172a;
  --surface: #1e293b;
  --surface-soft: #0f172a;
  --border-soft: #334155;
  --text-strong: #f1f5f9;
  --text-normal: #cbd5e1;
  --text-soft: #94a3b8;
  --accent: #22c55e;
  --shadow-soft: 0 18px 42px rgba(0, 0, 0, 0.45);
  --shadow-hover: 0 24px 52px rgba(0, 0, 0, 0.55);
}

html,
body,
#app {
  margin: 0;
  width: 100%;
  height: 100%;
  background: var(--app-bg);
  font-family:
    "Segoe UI Variable",
    "PingFang SC",
    "Hiragino Sans GB",
    "Microsoft YaHei UI",
    sans-serif;
}

body {
  color: var(--text-strong);
}

.app-shell {
  position: relative;
  height: 100vh;
  background:
    radial-gradient(circle at top left, rgba(23, 133, 108, 0.08), transparent 28%),
    radial-gradient(circle at 85% 12%, rgba(59, 130, 246, 0.08), transparent 20%),
    linear-gradient(180deg, #f7fafc 0%, #f3f6fb 100%);
}

:root.dark .app-shell {
  background:
    radial-gradient(circle at top left, rgba(34, 197, 94, 0.08), transparent 28%),
    radial-gradient(circle at 85% 12%, rgba(59, 130, 246, 0.08), transparent 20%),
    linear-gradient(180deg, #0f172a 0%, #0f172a 100%);
}

.app-shell::before {
  content: "";
  position: absolute;
  inset: 0;
  pointer-events: none;
  opacity: 0.45;
  background-image:
    linear-gradient(rgba(20, 32, 51, 0.03) 1px, transparent 1px),
    linear-gradient(90deg, rgba(20, 32, 51, 0.03) 1px, transparent 1px);
  background-size: 36px 36px;
}

.app-sider {
  border-right: 1px solid var(--border-soft);
}

.brand-panel {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 26px 22px 18px;
}

.brand-mark {
  display: grid;
  place-items: center;
  width: 48px;
  height: 48px;
  border-radius: 16px;
  background: linear-gradient(135deg, #17856c 0%, #56b79c 100%);
  color: #fff;
  font-size: 15px;
  font-weight: 800;
  letter-spacing: 0.08em;
  box-shadow: 0 16px 30px rgba(23, 133, 108, 0.24);
}

.brand-title {
  font-size: 18px;
  font-weight: 800;
  font-family:
    "Georgia",
    "Songti SC",
    "STSong",
    serif;
  color: var(--text-strong);
}

.brand-subtitle {
  display: block;
  margin-top: 4px;
  line-height: 1.5;
}

.sider-note {
  margin: 0 18px 14px;
  padding: 16px;
  border-radius: 22px;
  background:
    linear-gradient(135deg, rgba(255, 255, 255, 0.96) 0%, rgba(247, 251, 250, 0.96) 100%);
  border: 1px solid rgba(23, 133, 108, 0.12);
  box-shadow: 0 14px 28px rgba(23, 133, 108, 0.08);
}

:root.dark .sider-note {
  background: linear-gradient(135deg, rgba(30, 41, 59, 0.94) 0%, rgba(15, 23, 42, 0.94) 100%);
  border: 1px solid rgba(71, 85, 105, 0.7);
  box-shadow: 0 16px 30px rgba(0, 0, 0, 0.32);
}

.app-menu {
  padding: 0 10px 16px;
}

.app-main {
  position: relative;
  background: transparent;
}

.app-topbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 16px;
  padding: 24px 28px 8px;
}

.topbar-kicker {
  margin-bottom: 6px;
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.18em;
  text-transform: uppercase;
  color: var(--accent);
}

.topbar-title {
  margin-bottom: 6px;
  font-size: 30px;
  font-weight: 800;
  font-family:
    "Georgia",
    "Times New Roman",
    "Songti SC",
    serif;
  letter-spacing: -0.02em;
  color: var(--text-strong);
}

.topbar-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  justify-content: flex-end;
  align-items: center;
}

.theme-toggle-card {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 14px;
  border-radius: 18px;
  background: rgba(255, 255, 255, 0.7);
  border: 1px solid var(--border-soft);
  backdrop-filter: blur(10px);
}

:root.dark .theme-toggle-card {
  background: rgba(30, 41, 59, 0.86);
}

.theme-toggle-card__copy {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.theme-toggle-card__label {
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--text-soft);
}

.theme-toggle-card__value {
  font-size: 13px;
  font-weight: 700;
  color: var(--text-strong);
}

.view-scroll {
  height: calc(100vh - 104px);
  overflow: auto;
  padding: 12px 28px 28px;
  box-sizing: border-box;
}

.page-shell {
  display: flex;
  flex-direction: column;
  gap: 22px;
  max-width: 1320px;
  margin: 0 auto;
  animation: page-enter 0.28s ease;
}

.page-hero {
  padding: 28px;
  border-radius: var(--radius-xl);
  background:
    radial-gradient(circle at top right, rgba(59, 130, 246, 0.1), transparent 24%),
    radial-gradient(circle at 8% 18%, rgba(23, 133, 108, 0.08), transparent 18%),
    linear-gradient(135deg, #ffffff 0%, #f8fbff 100%);
  border: 1px solid var(--border-soft);
  box-shadow: var(--shadow-soft);
}

:root.dark .page-hero {
  background:
    radial-gradient(circle at top right, rgba(59, 130, 246, 0.18), transparent 24%),
    radial-gradient(circle at 8% 18%, rgba(34, 197, 94, 0.12), transparent 18%),
    linear-gradient(135deg, rgba(30, 41, 59, 0.96) 0%, rgba(15, 23, 42, 0.98) 100%);
  border: 1px solid rgba(71, 85, 105, 0.72);
}

.page-hero__title {
  font-size: 30px;
  font-weight: 800;
  font-family:
    "Georgia",
    "Times New Roman",
    "Songti SC",
    serif;
  letter-spacing: -0.02em;
  color: var(--text-strong);
}

.page-hero__desc {
  margin-top: 10px;
  line-height: 1.7;
  color: var(--text-normal);
}

.surface-card {
  border-radius: var(--radius-xl);
  overflow: hidden;
}

.interactive-card {
  transition:
    transform 0.22s ease,
    box-shadow 0.22s ease,
    border-color 0.22s ease;
}

.interactive-card:hover {
  transform: translateY(-2px);
  box-shadow: var(--shadow-hover);
}

.section-head {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 12px;
  margin-bottom: 16px;
}

.section-head__title {
  font-size: 20px;
  font-weight: 800;
  color: var(--text-strong);
}

.section-head__desc {
  margin-top: 6px;
  color: var(--text-normal);
  line-height: 1.7;
}

.metric-card {
  position: relative;
  padding: 18px 18px 16px;
  border-radius: var(--radius-lg);
  background: var(--surface);
  border: 1px solid var(--border-soft);
  box-shadow: var(--shadow-soft);
}

.metric-card__label {
  font-size: 13px;
  color: var(--text-soft);
}

.metric-card__value {
  margin-top: 10px;
  font-size: 26px;
  font-weight: 800;
  color: var(--text-strong);
}

.metric-card__hint {
  margin-top: 8px;
  font-size: 12px;
  color: var(--text-soft);
}

.filter-bar {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  padding: 14px;
  border-radius: var(--radius-lg);
  background: rgba(255, 255, 255, 0.82);
  border: 1px solid var(--border-soft);
  backdrop-filter: blur(12px);
}

:root.dark .filter-bar {
  background: rgba(15, 23, 42, 0.86);
  border: 1px solid rgba(51, 65, 85, 0.92);
}

@keyframes page-enter {
  from {
    opacity: 0;
    transform: translateY(8px);
  }

  to {
    opacity: 1;
    transform: translateY(0);
  }
}

@media (max-width: 1024px) {
  .app-topbar {
    align-items: flex-start;
    flex-direction: column;
  }

  .view-scroll {
    padding: 12px 18px 18px;
  }

  .page-hero {
    padding: 22px;
  }
}
</style>
