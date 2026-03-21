import { invoke } from "@tauri-apps/api/core";
import { ref } from "vue";
import type { AppConfig } from "@/types";

export function useConfig() {
  const config = ref<AppConfig | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function loadConfig(): Promise<AppConfig | null> {
    loading.value = true;
    error.value = null;
    try {
      const result = await invoke<AppConfig>("load_config");
      config.value = result;
      return result;
    } catch (e: any) {
      error.value = typeof e === "string" ? e : e.message || String(e);
      return null;
    } finally {
      loading.value = false;
    }
  }

  async function saveConfig(newConfig: AppConfig): Promise<boolean> {
    loading.value = true;
    error.value = null;
    try {
      await invoke("save_config", { config: newConfig });
      config.value = newConfig;
      return true;
    } catch (e: any) {
      error.value = typeof e === "string" ? e : e.message || String(e);
      return false;
    } finally {
      loading.value = false;
    }
  }

  async function testAiConfig(currentConfig: AppConfig): Promise<string | null> {
    loading.value = true;
    error.value = null;
    try {
      return await invoke<string>("test_ai_config", { config: currentConfig });
    } catch (e: any) {
      error.value = typeof e === "string" ? e : e.message || String(e);
      return null;
    } finally {
      loading.value = false;
    }
  }

  async function listAiModels(currentConfig: AppConfig): Promise<string[] | null> {
    loading.value = true;
    error.value = null;
    try {
      return await invoke<string[]>("list_ai_models", { config: currentConfig });
    } catch (e: any) {
      error.value = typeof e === "string" ? e : e.message || String(e);
      return null;
    } finally {
      loading.value = false;
    }
  }

  return {
    config,
    loading,
    error,
    loadConfig,
    saveConfig,
    testAiConfig,
    listAiModels,
  };
}
