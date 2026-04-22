import { invoke } from "@tauri-apps/api/core";
import { ref } from "vue";
import type {
  MigrationAdvisorOutput,
  MigrationExecutionRecord,
  MigrationPlan,
  MigrationPlanReview,
} from "@/types";

export interface MigrationExecutionRequest {
  plan: MigrationPlan;
  selectedActionIds: string[];
  dryRun: boolean;
}

export function useMigration() {
  const loading = ref(false);
  const executing = ref(false);
  const advice = ref<MigrationAdvisorOutput | null>(null);
  const record = ref<MigrationExecutionRecord | null>(null);
  const review = ref<MigrationPlanReview | null>(null);
  const history = ref<MigrationExecutionRecord[]>([]);
  const error = ref<string | null>(null);

  async function loadAdvice(targetRoot: string | null): Promise<MigrationAdvisorOutput | null> {
    loading.value = true;
    error.value = null;

    try {
      const result = await invoke<MigrationAdvisorOutput>("get_migration_advice", {
        targetRoot,
      });
      advice.value = result;
      return result;
    } catch (reason: any) {
      error.value = typeof reason === "string" ? reason : reason?.message || String(reason);
      return null;
    } finally {
      loading.value = false;
    }
  }

  async function executePlan(
    request: MigrationExecutionRequest
  ): Promise<MigrationExecutionRecord | null> {
    executing.value = true;
    error.value = null;

    try {
      const result = await invoke<MigrationExecutionRecord>("execute_migration_plan", {
        request,
      });
      record.value = result;
      return result;
    } catch (reason: any) {
      error.value = typeof reason === "string" ? reason : reason?.message || String(reason);
      return null;
    } finally {
      executing.value = false;
    }
  }

  async function loadHistory(): Promise<MigrationExecutionRecord[]> {
    error.value = null;
    try {
      const result = await invoke<MigrationExecutionRecord[]>("get_migration_run_history");
      history.value = result;
      return result;
    } catch (reason: any) {
      error.value = typeof reason === "string" ? reason : reason?.message || String(reason);
      return [];
    }
  }

  async function rollbackRun(runId: string): Promise<MigrationExecutionRecord | null> {
    executing.value = true;
    error.value = null;

    try {
      const result = await invoke<MigrationExecutionRecord>("rollback_migration_run", { runId });
      record.value = result;
      return result;
    } catch (reason: any) {
      error.value = typeof reason === "string" ? reason : reason?.message || String(reason);
      return null;
    } finally {
      executing.value = false;
    }
  }

  async function refinePlan(plan: MigrationPlan): Promise<MigrationPlanReview | null> {
    loading.value = true;
    error.value = null;

    try {
      const result = await invoke<MigrationPlanReview>("refine_migration_plan_with_ai", {
        plan,
      });
      review.value = result;
      return result;
    } catch (reason: any) {
      error.value = typeof reason === "string" ? reason : reason?.message || String(reason);
      return null;
    } finally {
      loading.value = false;
    }
  }

  return {
    loading,
    executing,
    advice,
    record,
    review,
    history,
    error,
    loadAdvice,
    executePlan,
    loadHistory,
    rollbackRun,
    refinePlan,
  };
}
