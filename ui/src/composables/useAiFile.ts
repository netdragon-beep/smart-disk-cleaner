import { invoke } from "@tauri-apps/api/core";
import { ref } from "vue";
import type { AppConfig, FileAiInsight } from "@/types";

export function useAiFile() {
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function requestFileInsight(
    path: string,
    config?: AppConfig | null
  ): Promise<FileAiInsight> {
    return await invoke<FileAiInsight>("explain_file_with_ai", { path, config });
  }

  async function explainFile(
    path: string,
    config?: AppConfig | null
  ): Promise<FileAiInsight | null> {
    loading.value = true;
    error.value = null;
    try {
      return await invoke<FileAiInsight>("explain_file_with_ai", { path, config });
    } catch (e: any) {
      error.value = typeof e === "string" ? e : e.message || String(e);
      return null;
    } finally {
      loading.value = false;
    }
  }

  return {
    loading,
    error,
    requestFileInsight,
    explainFile,
  };
}
