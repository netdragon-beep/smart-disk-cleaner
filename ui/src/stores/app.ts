import { defineStore } from "pinia";
import { ref } from "vue";
import type { ScanReport, OperationLogEntry, AppConfig } from "@/types";

export const useAppStore = defineStore("app", () => {
  const report = ref<ScanReport | null>(null);
  const history = ref<OperationLogEntry[]>([]);
  const config = ref<AppConfig | null>(null);
  const theme = ref<"light" | "dark">("dark");

  function setReport(r: ScanReport | null) {
    report.value = r;
  }

  function addHistory(entries: OperationLogEntry[]) {
    history.value.push(...entries);
  }

  function setHistory(entries: OperationLogEntry[]) {
    history.value = entries;
  }

  function setConfig(c: AppConfig) {
    config.value = c;
    if (c.theme === "light" || c.theme === "dark") {
      theme.value = c.theme;
    }
  }

  function toggleTheme() {
    theme.value = theme.value === "dark" ? "light" : "dark";
  }

  return {
    report,
    history,
    config,
    theme,
    setReport,
    addHistory,
    setHistory,
    setConfig,
    toggleTheme,
  };
});
