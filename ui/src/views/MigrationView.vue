<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import {
  NAlert,
  NButton,
  NCard,
  NCheckbox,
  NEmpty,
  NInput,
  NList,
  NListItem,
  NModal,
  NSpace,
  NSwitch,
  NTag,
  NText,
} from "naive-ui";
import { useAppStore } from "@/stores/app";
import { useCleanup } from "@/composables/useCleanup";
import { type MigrationExecutionRequest, useMigration } from "@/composables/useMigration";
import { useProcesses } from "@/composables/useProcesses";
import type {
  BlockingProcessInfo,
  MigrationAction,
  MigrationExecutionRecord,
  MigrationPlan,
  MigrationPlanReview,
  MigrationRunStatus,
  RiskLevel,
} from "@/types";

const TEXT = {
  noReport: "还没有扫描结果，请先扫描 C 盘或用户目录。",
  goScan: "去扫描",
  title: "AI 迁移计划",
  subtitle:
    "迁移助手会先生成结构化计划，再执行移动、配置调整和兼容链接等动作。若迁移失败是因为文件被占用，可以直接查看占用进程并重试。",
  targetRoot: "迁移目标根目录",
  refresh: "重新生成计划",
  loading: "正在生成迁移计划...",
  dryRun: "模拟执行",
  summary: "计划摘要",
  plans: "迁移计划",
  docs: "依据来源",
  actions: "执行动作",
  verification: "验证步骤",
  aiRefine: "AI 重规划",
  aiReview: "AI 规划结果",
  aiDocs: "文档摘要",
  aiCases: "历史案例",
  sourcePath: "源路径",
  targetPath: "推荐目标路径",
  rationale: "迁移理由",
  execute: "执行计划",
  confirm: "确认执行计划",
  executionHistory: "执行记录",
  rollback: "一键回滚",
  blockers: "占用进程",
  blockerHint: "如果迁移失败是因为文件被占用，可以直接查看是谁占用了它，结束后再重试当前迁移。",
  blockerEmpty: "没有查到明确的占用进程，可能占用已经解除或系统暂未返回结果。",
  blockerLoading: "正在查找占用进程...",
  blockerFind: "查找占用进程",
  blockerTerminate: "结束进程",
  blockerRetry: "重试当前迁移",
  close: "关闭",
  noItems: "当前没有可展示的迁移计划。",
  selectedActions: "已选动作",
  latestRun: "最近一次执行结果",
  checkpoints: "回滚检查点",
  logs: "执行日志",
  emptyHistory: "还没有迁移执行记录。",
  supportLevel: "支持方式",
  planTags: "计划标签",
  required: "必做",
  statusBlocker: "发现文件占用问题，可继续处理",
  targetRootHint: "建议选择空间更充足的其他盘符作为目标根目录。",
} as const;

const router = useRouter();
const store = useAppStore();
const { getPathBlockers } = useCleanup();
const { terminateProcess, terminating, terminateError } = useProcesses();
const {
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
} = useMigration();

const targetRoot = ref("D:/SmartDiskCleanerMigration");
const dryRun = ref(true);
const showConfirm = ref(false);
const showReview = ref(false);
const showRecord = ref(false);
const showBlockers = ref(false);
const selectedPlan = ref<MigrationPlan | null>(null);
const selectedActionIds = ref<string[]>([]);
const blockerPath = ref("");
const blockers = ref<BlockingProcessInfo[]>([]);
const blockerLoading = ref(false);

const report = computed(() => store.report);
const plans = computed(() => advice.value?.plans ?? []);
const activePlan = computed(() => review.value?.plan ?? selectedPlan.value);

onMounted(async () => {
  if (report.value) {
    await Promise.all([refreshPlans(), loadHistory()]);
  }
});

async function refreshPlans() {
  await loadAdvice(targetRoot.value.trim() || null);
}

function openConfirm(plan: MigrationPlan) {
  selectedPlan.value = plan;
  selectedActionIds.value = plan.actions
    .filter((action) => action.required || action.enabledByDefault)
    .map((action) => action.id);
  showConfirm.value = true;
}

async function handleExecute() {
  if (!activePlan.value) return;
  const request: MigrationExecutionRequest = {
    plan: activePlan.value,
    selectedActionIds: selectedActionIds.value,
    dryRun: dryRun.value,
  };
  showConfirm.value = false;
  const result = await executePlan(request);
  if (result) {
    showRecord.value = true;
    await loadHistory();
  }
}

async function handleRefine(plan: MigrationPlan) {
  const result = await refinePlan(plan);
  if (result) {
    selectedPlan.value = result.plan;
    selectedActionIds.value = result.plan.actions
      .filter((action) => action.required || action.enabledByDefault)
      .map((action) => action.id);
    showReview.value = true;
  }
}

async function handleRollback(run: MigrationExecutionRecord) {
  const result = await rollbackRun(run.runId);
  if (result) {
    showRecord.value = true;
    await loadHistory();
  }
}

function actionChecked(action: MigrationAction) {
  return selectedActionIds.value.includes(action.id);
}

function toggleAction(action: MigrationAction, checked: boolean) {
  if (action.required) return;
  if (checked && !selectedActionIds.value.includes(action.id)) {
    selectedActionIds.value = [...selectedActionIds.value, action.id];
  } else if (!checked) {
    selectedActionIds.value = selectedActionIds.value.filter((id) => id !== action.id);
  }
}

function riskTagType(value: RiskLevel): "success" | "warning" | "error" {
  if (value === "low") return "success";
  if (value === "medium") return "warning";
  return "error";
}

function riskLabel(value: RiskLevel) {
  if (value === "low") return "低风险";
  if (value === "medium") return "中风险";
  return "高风险";
}

function statusType(value: MigrationRunStatus): "default" | "success" | "warning" | "error" {
  if (value === "dry_run") return "default";
  if (value === "succeeded" || value === "rolled_back") return "success";
  return "error";
}

function statusLabel(value: MigrationRunStatus) {
  if (value === "dry_run") return "模拟执行";
  if (value === "succeeded") return "执行成功";
  if (value === "rolled_back") return "已回滚";
  return "执行失败";
}

function supportLabel(value: MigrationPlan["supportLevel"]) {
  if (value === "one_click") return "一键迁移";
  if (value === "guided") return "引导处理";
  return "手动处理";
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function canRollback(run: MigrationExecutionRecord) {
  return !run.dryRun && run.status !== "rolled_back";
}

function reviewStatusText(value: MigrationPlanReview | null) {
  if (!value) return "";
  if (value.usedFallback) {
    return value.fallbackReason
      ? `AI 重规划失败，已回退到本地计划：${value.fallbackReason}`
      : "AI 重规划失败，已回退到本地计划。";
  }
  if (value.source.startsWith("remote:")) {
    return `已使用 ${value.source} 完成结构化重规划。`;
  }
  return "当前展示的是本地计划。";
}

function hasBlockingFailure(run: MigrationExecutionRecord | null) {
  if (!run) return false;
  return run.logs.some(
    (log) =>
      !log.success &&
      (log.diagnosis?.code === "in_use_by_another_process" ||
        log.diagnosis?.code === "locked_region")
  );
}

async function openBlockersFromRecord() {
  if (!record.value) return;
  const failedEntry = record.value.logs.find(
    (log) =>
      !log.success &&
      (log.diagnosis?.code === "in_use_by_another_process" ||
        log.diagnosis?.code === "locked_region")
  );
  if (!failedEntry) return;

  blockerPath.value = failedEntry.path;
  blockers.value = [];
  blockerLoading.value = true;
  showBlockers.value = true;
  try {
    blockers.value = await getPathBlockers(failedEntry.path);
  } finally {
    blockerLoading.value = false;
  }
}

async function handleTerminateBlocker(pid: number) {
  await terminateProcess(pid);
  if (!terminateError.value && blockerPath.value) {
    blockers.value = await getPathBlockers(blockerPath.value);
  }
}

async function retryCurrentPlan() {
  if (!activePlan.value) return;
  const request: MigrationExecutionRequest = {
    plan: activePlan.value,
    selectedActionIds: selectedActionIds.value,
    dryRun: dryRun.value,
  };
  const result = await executePlan(request);
  if (result) {
    showBlockers.value = false;
    showRecord.value = true;
    await loadHistory();
  }
}
</script>

<template>
  <div v-if="!report">
    <n-empty :description="TEXT.noReport">
      <template #extra>
        <n-button @click="router.push({ name: 'scan' })">{{ TEXT.goScan }}</n-button>
      </template>
    </n-empty>
  </div>

  <div v-else class="page-shell migration-page">
    <section class="page-hero">
      <n-space vertical :size="18">
        <div>
          <div class="migration-hero-kicker">Migration Atelier</div>
          <div class="page-hero__title">{{ TEXT.title }}</div>
          <div class="page-hero__desc">{{ TEXT.subtitle }}</div>
        </div>

        <div class="migration-hero-grid">
          <div class="migration-target-card">
            <div class="migration-target-card__label">{{ TEXT.targetRoot }}</div>
            <div class="migration-target-card__row">
              <n-input
                v-model:value="targetRoot"
                :placeholder="TEXT.targetRoot"
                class="migration-target-card__input"
              />
              <n-button type="primary" :loading="loading" @click="refreshPlans">
                {{ TEXT.refresh }}
              </n-button>
            </div>
            <div class="migration-target-card__hint">{{ TEXT.targetRootHint }}</div>
          </div>

          <div class="metric-card">
            <div class="metric-card__label">执行模式</div>
            <div class="metric-card__value">{{ dryRun ? "模拟" : "正式" }}</div>
            <div class="metric-card__hint">迁移前建议先做一轮模拟执行</div>
            <div style="margin-top: 12px">
              <n-switch v-model:value="dryRun" />
            </div>
          </div>
        </div>
      </n-space>
    </section>

    <section class="metric-grid">
      <div class="metric-card">
        <div class="metric-card__label">计划数量</div>
        <div class="metric-card__value">{{ plans.length }}</div>
        <div class="metric-card__hint">当前可展示的迁移方案</div>
      </div>
      <div class="metric-card">
        <div class="metric-card__label">执行历史</div>
        <div class="metric-card__value">{{ history.length }}</div>
        <div class="metric-card__hint">可用于回滚与复盘</div>
      </div>
      <div class="metric-card">
        <div class="metric-card__label">AI 规划来源</div>
        <div class="metric-card__value">{{ advice?.source?.startsWith("remote") ? "远程" : "本地" }}</div>
        <div class="metric-card__hint">当前摘要由规则或远程模型生成</div>
      </div>
    </section>

    <n-alert v-if="error" type="error">
      {{ error }}
    </n-alert>

    <n-card class="surface-card interactive-card">
      <template #header>
        <div class="section-head">
          <div>
            <div class="section-head__title">{{ TEXT.summary }}</div>
            <div class="section-head__desc">先给用户一个整体结论，再进入具体计划。</div>
          </div>
        </div>
      </template>
      <div class="soft-panel">
        <n-text v-if="loading">{{ TEXT.loading }}</n-text>
        <n-text v-else style="white-space: pre-wrap">
          {{ advice?.summary || TEXT.noItems }}
        </n-text>
      </div>
    </n-card>

    <n-card class="surface-card interactive-card">
      <template #header>
        <div class="section-head">
          <div>
            <div class="section-head__title">{{ TEXT.plans }} ({{ plans.length }})</div>
            <div class="section-head__desc">每个计划都给出源路径、目标路径、动作清单和验证步骤。</div>
          </div>
        </div>
      </template>

      <n-empty v-if="!loading && plans.length === 0" :description="TEXT.noItems" />
      <div v-else class="migration-plan-list">
        <div v-for="plan in plans" :key="plan.id" class="migration-plan-card interactive-card">
          <div class="migration-plan-card__header">
            <div>
              <div class="migration-plan-card__title">{{ plan.title }}</div>
              <div class="migration-plan-card__meta">
                {{ plan.summary }} · {{ formatBytes(plan.estimatedSize) }}
              </div>
            </div>
            <n-space>
              <n-tag :type="riskTagType(plan.risk)" round>{{ riskLabel(plan.risk) }}</n-tag>
              <n-tag round>{{ supportLabel(plan.supportLevel) }}</n-tag>
            </n-space>
          </div>

          <div class="migration-plan-card__paths">
            <div class="soft-panel">
              <div class="migration-plan-card__label">{{ TEXT.sourcePath }}</div>
              <div class="migration-plan-card__value">{{ plan.sourcePath }}</div>
            </div>
            <div class="soft-panel">
              <div class="migration-plan-card__label">{{ TEXT.targetPath }}</div>
              <div class="migration-plan-card__value">{{ plan.recommendedTargetPath }}</div>
            </div>
          </div>

          <div class="soft-panel">
            <div class="migration-plan-card__label">{{ TEXT.rationale }}</div>
            <div class="migration-plan-card__value">{{ plan.rationale }}</div>
          </div>

          <div v-if="plan.tags.length > 0" class="migration-plan-card__tags">
            <div class="migration-plan-card__label">{{ TEXT.planTags }}</div>
            <n-space>
              <n-tag v-for="tag in plan.tags" :key="`${plan.id}-${tag}`" size="small" round>
                {{ tag }}
              </n-tag>
            </n-space>
          </div>

          <div class="migration-plan-card__section">
            <div class="migration-plan-card__label">{{ TEXT.docs }}</div>
            <div class="migration-stack-list">
              <div v-for="doc in plan.docSources" :key="`${plan.id}-${doc.title}`" class="migration-stack-item">
                <div class="migration-stack-item__title">{{ doc.title }}</div>
                <n-text depth="3">{{ doc.note }}</n-text>
                <n-text v-if="doc.uri" depth="3">{{ doc.uri }}</n-text>
              </div>
            </div>
          </div>

          <div class="migration-plan-card__section">
            <div class="migration-plan-card__label">{{ TEXT.actions }}</div>
            <div class="migration-stack-list">
              <div v-for="action in plan.actions" :key="action.id" class="migration-stack-item">
                <div class="migration-stack-item__title">
                  {{ action.title }}
                  <n-tag v-if="action.required" size="small" type="error" round style="margin-left: 8px">
                    {{ TEXT.required }}
                  </n-tag>
                </div>
                <n-text depth="3">{{ action.detail }}</n-text>
              </div>
            </div>
          </div>

          <div class="migration-plan-card__section">
            <div class="migration-plan-card__label">{{ TEXT.verification }}</div>
            <div class="migration-stack-list">
              <div
                v-for="(step, index) in plan.verificationSteps"
                :key="`${plan.id}-verify-${index}`"
                class="migration-stack-item"
              >
                <div class="migration-stack-item__title">{{ index + 1 }}. {{ step.title }}</div>
                <n-text depth="3">{{ step.detail }}</n-text>
              </div>
            </div>
          </div>

          <div class="migration-plan-card__footer">
            <n-button secondary :loading="loading" @click="handleRefine(plan)">
              {{ TEXT.aiRefine }}
            </n-button>
            <n-button type="primary" :loading="executing" @click="openConfirm(plan)">
              {{ TEXT.execute }}
            </n-button>
          </div>
        </div>
      </div>
    </n-card>

    <n-card class="surface-card interactive-card">
      <template #header>
        <div class="section-head">
          <div>
            <div class="section-head__title">{{ TEXT.executionHistory }} ({{ history.length }})</div>
            <div class="section-head__desc">回滚和失败排查都依赖这里的历史记录。</div>
          </div>
        </div>
      </template>

      <n-empty v-if="history.length === 0" :description="TEXT.emptyHistory" />
      <div v-else class="migration-history-list">
        <div v-for="run in history" :key="run.runId" class="migration-history-card">
          <div class="migration-history-card__top">
            <div>
              <div class="migration-history-card__title">{{ run.title }}</div>
              <div class="migration-history-card__meta">{{ run.runId }}</div>
              <div class="migration-history-card__meta">{{ run.startedAt }} -> {{ run.finishedAt }}</div>
            </div>
            <n-space>
              <n-tag :type="statusType(run.status)" round>{{ statusLabel(run.status) }}</n-tag>
              <n-button
                v-if="canRollback(run)"
                secondary
                type="warning"
                :loading="executing"
                @click="handleRollback(run)"
              >
                {{ TEXT.rollback }}
              </n-button>
            </n-space>
          </div>
        </div>
      </div>
    </n-card>

    <n-modal
      v-model:show="showConfirm"
      preset="card"
      :title="TEXT.confirm"
      style="width: min(780px, calc(100vw - 32px))"
    >
      <n-space v-if="activePlan" vertical :size="12">
        <div style="font-size: 18px; font-weight: 800">{{ activePlan.title }}</div>
        <n-text depth="3">{{ TEXT.selectedActions }}：{{ selectedActionIds.length }}</n-text>

        <div class="migration-stack-list">
          <div v-for="action in activePlan.actions" :key="action.id" class="migration-stack-item">
            <n-checkbox
              :checked="actionChecked(action)"
              :disabled="action.required"
              @update:checked="(checked) => toggleAction(action, checked)"
            >
              {{ action.title }}
            </n-checkbox>
            <n-text depth="3" style="display: block; margin-left: 28px">
              {{ action.detail }}
            </n-text>
          </div>
        </div>
      </n-space>

      <template #footer>
        <n-space justify="end">
          <n-button @click="showConfirm = false">{{ TEXT.close }}</n-button>
          <n-button type="primary" :loading="executing" @click="handleExecute">
            {{ TEXT.execute }}
          </n-button>
        </n-space>
      </template>
    </n-modal>

    <n-modal
      v-model:show="showReview"
      preset="card"
      :title="TEXT.aiReview"
      style="width: min(860px, calc(100vw - 32px))"
    >
      <n-space v-if="review" vertical :size="12">
        <n-alert :type="review.usedFallback ? 'warning' : 'success'">
          {{ reviewStatusText(review) }}
        </n-alert>

        <div class="migration-stack-section">
          <div class="migration-plan-card__label">{{ TEXT.aiDocs }}</div>
          <div class="migration-stack-list">
            <div v-for="doc in review.docExcerpts" :key="`${doc.title}-${doc.uri}`" class="migration-stack-item">
              <div class="migration-stack-item__title">{{ doc.title }}</div>
              <n-text depth="3" style="display: block; white-space: pre-wrap">
                {{ doc.excerpt }}
              </n-text>
            </div>
          </div>
        </div>

        <div class="migration-stack-section">
          <div class="migration-plan-card__label">{{ TEXT.aiCases }}</div>
          <div class="migration-stack-list">
            <div v-for="item in review.historicalCases" :key="item.runId" class="migration-stack-item">
              <div class="migration-stack-item__title">
                {{ item.title }} · {{ statusLabel(item.status) }}
              </div>
              <n-text depth="3" v-if="item.failureReason">
                {{ item.failureReason }}
              </n-text>
            </div>
          </div>
        </div>

        <div class="migration-stack-section">
          <div class="migration-plan-card__label">{{ TEXT.actions }}</div>
          <div class="migration-stack-list">
            <div v-for="action in review.plan.actions" :key="action.id" class="migration-stack-item">
              <div class="migration-stack-item__title">{{ action.title }}</div>
              <n-text depth="3">{{ action.detail }}</n-text>
            </div>
          </div>
        </div>
      </n-space>

      <template #footer>
        <n-space justify="end">
          <n-button @click="showReview = false">{{ TEXT.close }}</n-button>
        </n-space>
      </template>
    </n-modal>

    <n-modal
      v-model:show="showRecord"
      preset="card"
      :title="TEXT.latestRun"
      style="width: min(800px, calc(100vw - 32px))"
    >
      <n-space v-if="record" vertical :size="12">
        <n-space>
          <n-tag :type="statusType(record.status)" round>{{ statusLabel(record.status) }}</n-tag>
          <n-tag v-if="hasBlockingFailure(record)" type="warning" round>
            {{ TEXT.statusBlocker }}
          </n-tag>
        </n-space>

        <n-text v-if="record.failureReason" type="error">
          {{ record.failureReason }}
        </n-text>

        <div class="migration-stack-section">
          <div class="migration-plan-card__label">{{ TEXT.checkpoints }}</div>
          <div class="migration-stack-list">
            <div v-for="checkpoint in record.checkpoints" :key="checkpoint.key" class="migration-stack-item">
              <div class="migration-stack-item__title">{{ checkpoint.key }}</div>
              <n-text depth="3">{{ checkpoint.target }}</n-text>
            </div>
          </div>
        </div>

        <div class="migration-stack-section">
          <div class="migration-plan-card__label">{{ TEXT.logs }}</div>
          <div class="migration-stack-list">
            <div v-for="(log, index) in record.logs" :key="`${record.runId}-${index}`" class="migration-log-item">
              <n-tag :type="log.success ? 'success' : 'error'" size="small" round>
                {{ log.success ? "成功" : "失败" }}
              </n-tag>
              <n-text>{{ log.detail }}</n-text>
            </div>
          </div>
        </div>

        <n-button
          v-if="hasBlockingFailure(record)"
          type="warning"
          secondary
          @click="openBlockersFromRecord"
        >
          {{ TEXT.blockerFind }}
        </n-button>
      </n-space>

      <template #footer>
        <n-space justify="end">
          <n-button @click="showRecord = false">{{ TEXT.close }}</n-button>
        </n-space>
      </template>
    </n-modal>

    <n-modal
      v-model:show="showBlockers"
      preset="card"
      :title="TEXT.blockers"
      style="width: min(760px, calc(100vw - 32px))"
    >
      <n-space vertical :size="12">
        <n-alert type="warning">
          {{ TEXT.blockerHint }}
        </n-alert>
        <n-text style="word-break: break-all">{{ blockerPath }}</n-text>
        <n-text v-if="blockerLoading" depth="3">
          {{ TEXT.blockerLoading }}
        </n-text>
        <n-alert v-if="terminateError" type="error">
          {{ terminateError }}
        </n-alert>
        <n-empty
          v-if="!blockerLoading && blockers.length === 0"
          :description="TEXT.blockerEmpty"
        />
        <n-list v-else bordered>
          <n-list-item v-for="item in blockers" :key="item.pid">
            <n-space justify="space-between" align="center" style="width: 100%" :wrap="false">
              <div class="migration-blocker-card">
                <div class="migration-stack-item__title">{{ item.appName }}</div>
                <n-text depth="3">PID：{{ item.pid }}</n-text>
                <n-text v-if="item.serviceName" depth="3" style="display: block">
                  服务：{{ item.serviceName }}
                </n-text>
                <n-text depth="3" style="display: block">
                  可重启：{{ item.restartable ? "是" : "否" }}
                </n-text>
              </div>
              <n-button
                size="small"
                secondary
                type="error"
                :loading="terminating"
                @click="handleTerminateBlocker(item.pid)"
              >
                {{ TEXT.blockerTerminate }}
              </n-button>
            </n-space>
          </n-list-item>
        </n-list>
      </n-space>
      <template #footer>
        <n-space justify="end">
          <n-button @click="showBlockers = false">{{ TEXT.close }}</n-button>
          <n-button type="primary" :loading="executing" @click="retryCurrentPlan">
            {{ TEXT.blockerRetry }}
          </n-button>
        </n-space>
      </template>
    </n-modal>
  </div>
</template>

<style scoped>
.migration-page {
  gap: 18px;
}

.migration-hero-kicker {
  margin-bottom: 8px;
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.16em;
  text-transform: uppercase;
  color: var(--accent);
}

.migration-hero-grid {
  display: grid;
  grid-template-columns: minmax(0, 1.6fr) minmax(220px, 0.7fr);
  gap: 16px;
}

.migration-target-card {
  padding: 18px;
  border-radius: 20px;
  background: rgba(255, 255, 255, 0.84);
  border: 1px solid rgba(230, 235, 243, 0.94);
}

.migration-target-card__label,
.migration-plan-card__label {
  margin-bottom: 8px;
  font-size: 12px;
  font-weight: 700;
  color: var(--text-soft);
}

.migration-target-card__row {
  display: flex;
  gap: 12px;
}

.migration-target-card__input {
  flex: 1;
}

.migration-target-card__hint {
  margin-top: 10px;
  color: var(--text-normal);
  line-height: 1.7;
}

.migration-plan-list,
.migration-history-list {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.migration-plan-card,
.migration-history-card {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 18px;
  border-radius: 22px;
  border: 1px solid var(--border-soft);
  background: linear-gradient(135deg, #ffffff 0%, #fafcff 100%);
  box-shadow: 0 16px 34px rgba(15, 23, 42, 0.05);
}

.migration-plan-card__header,
.migration-history-card__top {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 16px;
}

.migration-plan-card__title,
.migration-history-card__title {
  font-size: 19px;
  font-weight: 800;
  color: var(--text-strong);
}

.migration-plan-card__meta,
.migration-history-card__meta {
  margin-top: 6px;
  color: var(--text-normal);
  line-height: 1.7;
}

.migration-plan-card__paths {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}

.migration-plan-card__value {
  line-height: 1.7;
  word-break: break-all;
  color: var(--text-normal);
}

.migration-plan-card__tags,
.migration-plan-card__section,
.migration-stack-section {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.migration-stack-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.migration-stack-item {
  padding: 14px 16px;
  border-radius: 16px;
  background: var(--surface-soft);
  border: 1px solid var(--border-soft);
  transition: transform 0.18s ease, border-color 0.18s ease;
}

.migration-stack-item:hover {
  transform: translateY(-1px);
  border-color: rgba(23, 133, 108, 0.24);
}

.migration-stack-item__title {
  margin-bottom: 6px;
  font-weight: 700;
  color: var(--text-strong);
}

.migration-plan-card__footer {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
}

.migration-log-item {
  display: flex;
  align-items: center;
  gap: 10px;
}

.migration-blocker-card {
  min-width: 0;
}

@media (max-width: 960px) {
  .migration-hero-grid,
  .migration-plan-card__paths {
    grid-template-columns: 1fr;
  }

  .migration-target-card__row,
  .migration-plan-card__header,
  .migration-history-card__top,
  .migration-plan-card__footer {
    flex-direction: column;
    align-items: stretch;
  }
}
</style>
