<script setup lang="ts">
import { computed, onErrorCaptured, onMounted, ref, watch } from "vue";
import { useRouter, useRoute } from "vue-router";
import {
  NAlert,
  NButton,
  NConfigProvider,
  NLayout,
  NLayoutSider,
  NMenu,
  NMessageProvider,
  NLayoutContent,
  NSpace,
  zhCN,
  dateZhCN,
  darkTheme,
  type MenuOption,
} from "naive-ui";
import { useAppStore } from "@/stores/app";
import { useConfig } from "@/composables/useConfig";

const router = useRouter();
const route = useRoute();
const store = useAppStore();
const { loadConfig } = useConfig();

const appTitle = "\u667A\u80FD\u78C1\u76D8\u6E05\u7406";
const renderEpoch = ref(0);
const routeError = ref("");

const themeOverride = computed(() =>
  store.theme === "dark" ? darkTheme : null
);
const viewKey = computed(() => `${route.fullPath}:${renderEpoch.value}`);

const menuOptions: MenuOption[] = [
  { label: "\u626B\u63CF", key: "scan" },
  { label: "\u7ED3\u679C", key: "results" },
  { label: "\u6E05\u7406", key: "cleanup" },
  { label: "\u5386\u53F2", key: "history" },
  { label: "\u8BBE\u7F6E", key: "settings" },
];

const activeKey = computed(() => (route.name as string) || "scan");

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
  routeError.value =
    error instanceof Error ? error.message : String(error ?? "\u672A\u77E5\u524D\u7AEF\u9519\u8BEF");
  return false;
});

onMounted(async () => {
  const cfg = await loadConfig();
  if (cfg) store.setConfig(cfg);
});
</script>

<template>
  <n-config-provider
    :theme="themeOverride"
    :locale="zhCN"
    :date-locale="dateZhCN"
  >
    <n-message-provider>
      <n-layout has-sider style="height: 100vh">
        <n-layout-sider
          bordered
          :width="200"
          :collapsed-width="0"
          show-trigger="bar"
          collapse-mode="width"
          content-style="padding: 12px 0;"
        >
          <div
            style="
              padding: 16px 24px;
              font-size: 18px;
              font-weight: 700;
              white-space: nowrap;
            "
          >
            {{ appTitle }}
          </div>
          <n-menu
            :value="activeKey"
            :options="menuOptions"
            @update:value="handleMenuUpdate"
          />
        </n-layout-sider>
        <n-layout-content content-style="padding: 24px;">
          <n-alert v-if="routeError" type="error" title="页面渲染失败">
            <n-space vertical :size="12">
              <div>{{ routeError }}</div>
              <n-button @click="reloadCurrentView" style="width: 180px">
                重新加载当前页面
              </n-button>
            </n-space>
          </n-alert>
          <router-view v-else :key="viewKey" />
        </n-layout-content>
      </n-layout>
    </n-message-provider>
  </n-config-provider>
</template>
