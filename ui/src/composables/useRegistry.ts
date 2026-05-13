import { invoke } from "@tauri-apps/api/core";
import { ref } from "vue";
import type {
  RegistryBackup,
  RegistryChangePreview,
  RegistryEntryRecord,
  RegistryIssue,
  RegistryRollbackRecord,
} from "@/types";

export function useRegistry() {
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function listStartupEntries(): Promise<RegistryEntryRecord[]> {
    loading.value = true;
    error.value = null;
    try {
      return await invoke<RegistryEntryRecord[]>("list_registry_startup_entries");
    } catch (reason: any) {
      error.value = typeof reason === "string" ? reason : reason.message || String(reason);
      return [];
    } finally {
      loading.value = false;
    }
  }

  async function listPathIssues(): Promise<RegistryIssue[]> {
    loading.value = true;
    error.value = null;
    try {
      return await invoke<RegistryIssue[]>("list_registry_path_issues");
    } catch (reason: any) {
      error.value = typeof reason === "string" ? reason : reason.message || String(reason);
      return [];
    } finally {
      loading.value = false;
    }
  }

  async function exportBackup(entryId: string): Promise<RegistryBackup | null> {
    error.value = null;
    try {
      return await invoke<RegistryBackup>("export_registry_backup", { entryId });
    } catch (reason: any) {
      error.value = typeof reason === "string" ? reason : reason.message || String(reason);
      return null;
    }
  }

  async function previewChange(entryId: string): Promise<RegistryChangePreview | null> {
    error.value = null;
    try {
      return await invoke<RegistryChangePreview>("preview_registry_change", { entryId });
    } catch (reason: any) {
      error.value = typeof reason === "string" ? reason : reason.message || String(reason);
      return null;
    }
  }

  async function applyChange(
    entryId: string,
    dryRun: boolean
  ): Promise<RegistryChangePreview | null> {
    error.value = null;
    try {
      return await invoke<RegistryChangePreview>("apply_registry_change", { entryId, dryRun });
    } catch (reason: any) {
      error.value = typeof reason === "string" ? reason : reason.message || String(reason);
      return null;
    }
  }

  async function rollbackChange(backupId: string): Promise<RegistryRollbackRecord | null> {
    error.value = null;
    try {
      return await invoke<RegistryRollbackRecord>("rollback_registry_change", { backupId });
    } catch (reason: any) {
      error.value = typeof reason === "string" ? reason : reason.message || String(reason);
      return null;
    }
  }

  return {
    loading,
    error,
    listStartupEntries,
    listPathIssues,
    exportBackup,
    previewChange,
    applyChange,
    rollbackChange,
  };
}
