import { invoke } from "@tauri-apps/api/core";
import { ref } from "vue";
import type { BlockingProcessInfo, OperationLogEntry } from "@/types";

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

  async function getPathBlockers(path: string): Promise<BlockingProcessInfo[]> {
    error.value = null;
    try {
      return await invoke<BlockingProcessInfo[]>("get_path_blockers", { path });
    } catch (e: any) {
      error.value = typeof e === "string" ? e : e.message || String(e);
      return [];
    }
  }

  return { executing, logs, error, executeCleanup, getHistory, getPathBlockers };
}
