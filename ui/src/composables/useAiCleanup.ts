import { invoke } from "@tauri-apps/api/core";
import { ref } from "vue";
import type { AiCleanupPlan, AppConfig } from "@/types";

export function useAiCleanup() {
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function generatePlan(config?: AppConfig | null): Promise<AiCleanupPlan | null> {
    loading.value = true;
    error.value = null;
    try {
      return await invoke<AiCleanupPlan>("generate_ai_cleanup_plan", { config });
    } catch (e: any) {
      error.value = typeof e === "string" ? e : e.message || String(e);
      return null;
    } finally {
      loading.value = false;
    }
  }

  async function openAppsAndFeatures(): Promise<boolean> {
    error.value = null;
    try {
      await invoke("open_apps_and_features");
      return true;
    } catch (e: any) {
      error.value = typeof e === "string" ? e : e.message || String(e);
      return false;
    }
  }

  return {
    loading,
    error,
    generatePlan,
    openAppsAndFeatures,
  };
}
