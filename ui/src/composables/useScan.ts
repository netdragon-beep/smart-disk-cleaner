import { invoke, Channel } from "@tauri-apps/api/core";
import { ref } from "vue";
import type { ScanReport, ProgressEvent } from "@/types";

export function useScan() {
  const scanning = ref(false);
  const report = ref<ScanReport | null>(null);
  const error = ref<string | null>(null);
  const progress = ref<ProgressEvent | null>(null);

  async function startScan(path: string): Promise<ScanReport | null> {
    scanning.value = true;
    error.value = null;
    progress.value = null;
    report.value = null;

    try {
      const onProgress = new Channel<ProgressEvent>();
      onProgress.onmessage = (evt: ProgressEvent) => {
        progress.value = evt;
      };

      const result = await invoke<ScanReport>("start_scan_v2", {
        path,
        onProgress,
      });
      report.value = result;
      return result;
    } catch (e: any) {
      error.value = typeof e === "string" ? e : e.message || String(e);
      return null;
    } finally {
      scanning.value = false;
    }
  }

  async function cancelScan() {
    try {
      await invoke("cancel_scan");
    } catch {
      // ignore
    }
  }

  async function getLatestScanReport(): Promise<ScanReport | null> {
    try {
      const result = await invoke<ScanReport>("get_latest_scan_report");
      report.value = result;
      return result;
    } catch (e: any) {
      error.value = typeof e === "string" ? e : e.message || String(e);
      return null;
    }
  }

  return { scanning, report, error, progress, startScan, cancelScan, getLatestScanReport };
}
