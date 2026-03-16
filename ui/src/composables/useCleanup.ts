import { invoke } from "@tauri-apps/api/core";
import { ref } from "vue";
import type { OperationLogEntry } from "@/types";

export function useCleanup() {
  const executing = ref(false);
  const logs = ref<OperationLogEntry[]>([]);
  const error = ref<string | null>(null);

  async function executeCleanup(
    paths: string[],
    mode: string,
    targetDir: string | null,
    dryRun: boolean
  ): Promise<OperationLogEntry[]> {
    executing.value = true;
    error.value = null;

    try {
      const result = await invoke<OperationLogEntry[]>("execute_cleanup", {
        paths,
        mode,
        targetDir,
        dryRun,
      });
      logs.value = result;
      return result;
    } catch (e: any) {
      error.value = typeof e === "string" ? e : e.message || String(e);
      return [];
    } finally {
      executing.value = false;
    }
  }

  async function getHistory(): Promise<OperationLogEntry[]> {
    try {
      const result = await invoke<OperationLogEntry[]>("get_operation_history");
      return result;
    } catch {
      return [];
    }
  }

  return { executing, logs, error, executeCleanup, getHistory };
}
