<script setup lang="ts">
import { computed, h, onErrorCaptured, onMounted, ref, watch } from "vue";
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
  NTooltip,
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

function buildMenuOption(label: string, key: string, description: string): MenuOption {
  return {
    key,
    label: () =>
      h(
        NTooltip,
        {
          delay: 600,
          placement: "right",
          style: "max-width: 260px;",
        },
        {
          trigger: () =>
            h(
              "div",
              {
                style:
                  "display: flex; align-items: center; min-width: 0; width: 100%;",
              },
              label
            ),
          default: () => description,
        }
      ),
  };
}

const menuOptions: MenuOption[] = [
  buildMenuOption("\u78C1\u76D8\u626B\u63CF", "scan", "\u9009\u62E9\u4E00\u4E2A\u78C1\u76D8\u6216\u76EE\u5F55\uFF0C\u5F00\u59CB\u5206\u6790\u7A7A\u95F4\u5360\u7528\u4E0E\u53EF\u6E05\u7406\u5185\u5BB9\u3002"),
  buildMenuOption("\u8FDB\u7A0B\u8BCA\u65AD", "processes", "\u67E5\u770B\u9AD8\u5360\u7528\u8FDB\u7A0B\uFF0C\u7406\u89E3\u5B83\u4EEC\u7684\u7528\u9014\uFF0C\u5E76\u51B3\u5B9A\u662F\u5426\u9002\u5408\u7ED3\u675F\u3002"),
  buildMenuOption("\u626B\u63CF\u7ED3\u679C", "results", "\u6309\u6587\u4EF6\u3001\u76EE\u5F55\u548C\u7C7B\u578B\u67E5\u770B\u672C\u6B21\u626B\u63CF\u53D1\u73B0\u7684\u95EE\u9898\u4E0E\u5360\u7528\u6982\u89C8\u3002"),
  buildMenuOption("\u6267\u884C\u6E05\u7406", "cleanup", "\u6839\u636E\u5EFA\u8BAE\u6267\u884C\u5220\u9664\u3001\u56DE\u6536\u7AD9\u79FB\u52A8\u6216\u5176\u4ED6\u6E05\u7406\u64CD\u4F5C\u3002"),
  buildMenuOption("\u64CD\u4F5C\u5386\u53F2", "history", "\u67E5\u770B\u4F60\u4E4B\u524D\u6267\u884C\u8FC7\u7684\u6E05\u7406\u8BB0\u5F55\u4E0E\u5904\u7406\u7ED3\u679C\u3002"),
  buildMenuOption("\u504F\u597D\u8BBE\u7F6E", "settings", "\u8C03\u6574 AI\u3001\u9608\u503C\u3001\u6392\u9664\u89C4\u5219\u548C\u754C\u9762\u504F\u597D\u3002"),
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
