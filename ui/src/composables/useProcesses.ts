import { invoke } from "@tauri-apps/api/core";
import { ref } from "vue";
import type { AppConfig, ProcessAiInsight, ProcessRecord } from "@/types";

export function useProcesses() {
  const processes = ref<ProcessRecord[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);

  const explaining = ref(false);
  const explainError = ref<string | null>(null);

  const terminating = ref(false);
  const terminateError = ref<string | null>(null);

  async function requestProcessInsight(
    pid: number,
    config?: AppConfig | null
  ): Promise<ProcessAiInsight> {
    return await invoke<ProcessAiInsight>("explain_process_with_ai", { pid, config });
  }

  async function loadProcesses(limit = 30): Promise<ProcessRecord[]> {
    loading.value = true;
    error.value = null;
    try {
      const result = await invoke<ProcessRecord[]>("list_top_processes", { limit });
      processes.value = result;
      return result;
    } catch (e: any) {
      error.value = typeof e === "string" ? e : e.message || String(e);
      processes.value = [];
      return [];
    } finally {
      loading.value = false;
    }
  }

  async function explainProcess(
    pid: number,
    config?: AppConfig | null
  ): Promise<ProcessAiInsight | null> {
    explaining.value = true;
    explainError.value = null;
    try {
      return await invoke<ProcessAiInsight>("explain_process_with_ai", { pid, config });
    } catch (e: any) {
      explainError.value = typeof e === "string" ? e : e.message || String(e);
      return null;
    } finally {
      explaining.value = false;
    }
  }

  async function terminateProcess(pid: number): Promise<string | null> {
    terminating.value = true;
    terminateError.value = null;
    try {
      return await invoke<string>("terminate_process", { pid });
    } catch (e: any) {
      terminateError.value = typeof e === "string" ? e : e.message || String(e);
      return null;
    } finally {
      terminating.value = false;
    }
  }

  return {
    processes,
    loading,
    error,
    explaining,
    explainError,
    terminating,
    terminateError,
    loadProcesses,
    requestProcessInsight,
    explainProcess,
    terminateProcess,
  };
}
