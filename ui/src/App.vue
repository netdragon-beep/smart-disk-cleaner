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
  NTag,
  NText,
  NTooltip,
  dateZhCN,
  zhCN,
  type GlobalThemeOverrides,
  type MenuOption,
} from "naive-ui";
import { useConfig } from "@/composables/useConfig";
import { useAppStore } from "@/stores/app";

const router = useRouter();
const route = useRoute();
const store = useAppStore();
const { loadConfig } = useConfig();

const appTitle = "智能磁盘整理助手";
const renderEpoch = ref(0);
const routeError = ref("");

const pageMeta: Record<string, { title: string; subtitle: string; badge: string }> = {
  scan: {
    title: "磁盘扫描",
    subtitle: "从目录选择、扫描进度到结果进入，保持流程清晰可感知。",
    badge: "入口页",
  },
  migration: {
    title: "迁移助手",
    subtitle: "把可迁移对象、计划动作、失败重试和回滚路径组织成一套连续操作。",
    badge: "AI 辅助",
  },
  processes: {
    title: "进程诊断",
    subtitle: "把资源占用、风险判断和结束建议放在同一个工作台里。",
    badge: "系统状态",
  },
  results: {
    title: "扫描结果",
    subtitle: "从应用、目录、文件和建议四个层级查看空间问题。",
    badge: "分析中心",
  },
  cleanup: {
    title: "执行清理",
    subtitle: "先筛选，再执行；遇到占用冲突时直接定位并处理。",
    badge: "执行页",
  },
  history: {
    title: "历史记录",
    subtitle: "回看过去的清理与迁移动作，便于复盘和继续处理。",
    badge: "审计",
  },
  settings: {
    title: "偏好设置",
    subtitle: "统一管理阈值、排除规则和 AI 配置。",
    badge: "配置",
  },
};

const currentPage = computed(
  () => pageMeta[(route.name as string) || "scan"] ?? pageMeta.scan
);
const viewKey = computed(() => `${route.fullPath}:${renderEpoch.value}`);
const activeKey = computed(() => (route.name as string) || "scan");

const themeOverrides: GlobalThemeOverrides = {
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
  Card: {
    color: "#ffffff",
    colorEmbedded: "#f7f9fc",
    borderColor: "#e6ebf3",
    borderRadius: "22px",
    boxShadow: "0 18px 42px rgba(15, 23, 42, 0.06)",
    titleTextColor: "#142033",
    textColor: "#516079",
  },
  DataTable: {
    thColor: "#f7f9fc",
    thColorHover: "#f0f4fa",
    tdColor: "#ffffff",
    tdColorHover: "#f8fbff",
    borderColor: "#e9eef5",
  },
  Input: {
    borderHover: "#b8d9cf",
    borderFocus: "#17856c",
    color: "#ffffff",
    boxShadowFocus: "0 0 0 3px rgba(23, 133, 108, 0.14)",
  },
  Select: {
    peers: {
      InternalSelection: {
        borderHover: "#b8d9cf",
        borderFocus: "#17856c",
        boxShadowFocus: "0 0 0 3px rgba(23, 133, 108, 0.14)",
      },
    },
  },
  Button: {
    borderRadiusMedium: "14px",
    borderRadiusSmall: "12px",
    fontWeight: "600",
  },
  Menu: {
    itemTextColorActive: "#17856c",
    itemTextColorHover: "#142033",
    itemTextColorActiveHover: "#17856c",
    itemTextColorChildActive: "#17856c",
    itemColorHover: "#f2f8f6",
    itemColorActive: "#ecf7f3",
    itemColorActiveHover: "#ecf7f3",
    arrowColorActive: "#17856c",
  },
  Tag: {
    borderRadius: "999px",
  },
  Alert: {
    borderRadius: "18px",
  },
  Modal: {
    borderRadius: "24px",
  },
};

function buildMenuOption(label: string, key: string, description: string): MenuOption {
  return {
    key,
    label: () =>
      h(
        NTooltip,
        {
          delay: 400,
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
  buildMenuOption("磁盘扫描", "scan", "选择磁盘或目录，开始空间分析。"),
  buildMenuOption("迁移助手", "migration", "识别可迁移内容并生成分步计划。"),
  buildMenuOption("进程诊断", "processes", "查看高占用进程并判断是否可以结束。"),
  buildMenuOption("扫描结果", "results", "从应用、文件和建议多个视角复核结果。"),
  buildMenuOption("执行清理", "cleanup", "批量执行清理，处理被占用文件。"),
  buildMenuOption("操作历史", "history", "查看过去执行记录和回滚结果。"),
  buildMenuOption("偏好设置", "settings", "调整阈值、排除规则和 AI 参数。"),
];

function handleMenuUpdate(key: string) {
  router.push({ name: key });
}

function reloadCurrentView() {
  routeError.value = "";
  renderEpoch.value += 1;
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

onMounted(async () => {
  const cfg = await loadConfig();
  if (cfg) {
    store.setConfig(cfg);
  }
});
</script>

<template>
  <n-config-provider :theme-overrides="themeOverrides" :locale="zhCN" :date-locale="dateZhCN">
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
                扫描、清理、迁移一体化工作台
              </n-text>
            </div>
          </div>

          <div class="sider-note">
            <n-tag size="small" type="success" round>白色主题</n-tag>
            <n-text depth="3">
              当前界面已切换为更轻、更清楚的卡片化布局，重点信息会更靠前。
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
                <div class="topbar-kicker">Smart Disk Studio</div>
                <div class="topbar-title">{{ currentPage.title }}</div>
                <n-text depth="3">{{ currentPage.subtitle }}</n-text>
              </div>
              <div class="topbar-meta">
                <n-tag round size="medium" type="success">White Editorial UI</n-tag>
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
  --surface-accent: linear-gradient(135deg, #ffffff 0%, #f7fbfa 100%);
  --border-soft: #e6ebf3;
  --border-strong: #dbe4ef;
  --text-strong: #142033;
  --text-normal: #516079;
  --text-soft: #7b879b;
  --accent: #17856c;
  --accent-soft: rgba(23, 133, 108, 0.12);
  --shadow-soft: 0 18px 42px rgba(15, 23, 42, 0.06);
  --shadow-hover: 0 24px 52px rgba(15, 23, 42, 0.1);
  --radius-xl: 24px;
  --radius-lg: 20px;
  --radius-md: 16px;
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
  position: relative;
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

.brand-copy {
  min-width: 0;
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
  position: relative;
  padding: 28px;
  border-radius: var(--radius-xl);
  background:
    radial-gradient(circle at top right, rgba(59, 130, 246, 0.1), transparent 24%),
    radial-gradient(circle at 8% 18%, rgba(23, 133, 108, 0.08), transparent 18%),
    linear-gradient(135deg, #ffffff 0%, #f8fbff 100%);
  border: 1px solid var(--border-soft);
  box-shadow: var(--shadow-soft);
  overflow: hidden;
}

.page-hero::after {
  content: "";
  position: absolute;
  right: -50px;
  top: -40px;
  width: 240px;
  height: 240px;
  border-radius: 999px;
  background:
    radial-gradient(circle, rgba(23, 133, 108, 0.12) 0%, rgba(23, 133, 108, 0) 68%);
  pointer-events: none;
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

.metric-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
  gap: 14px;
}

.metric-card {
  position: relative;
  padding: 18px 18px 16px;
  border-radius: var(--radius-lg);
  background: var(--surface);
  border: 1px solid var(--border-soft);
  box-shadow: var(--shadow-soft);
  overflow: hidden;
}

.metric-card::before {
  content: "";
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  height: 3px;
  background: linear-gradient(90deg, rgba(23, 133, 108, 0.9), rgba(59, 130, 246, 0.5));
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

.surface-card {
  border-radius: var(--radius-xl);
  overflow: hidden;
}

.soft-panel {
  padding: 16px;
  border-radius: var(--radius-lg);
  background: var(--surface-soft);
  border: 1px solid var(--border-soft);
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

.scroll-area::-webkit-scrollbar,
.view-scroll::-webkit-scrollbar {
  width: 10px;
  height: 10px;
}

.scroll-area::-webkit-scrollbar-thumb,
.view-scroll::-webkit-scrollbar-thumb {
  background: #d4dce8;
  border-radius: 999px;
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
