<script setup lang="ts">
import { computed, h, onMounted, ref } from "vue";
import {
  NAlert,
  NButton,
  NCard,
  NDataTable,
  NEmpty,
  NList,
  NListItem,
  NModal,
  NSpace,
  NSwitch,
  NTag,
  NText,
  useMessage,
  type DataTableColumns,
} from "naive-ui";
import { useRegistry } from "@/composables/useRegistry";
import { useCleanup } from "@/composables/useCleanup";
import { useAppStore } from "@/stores/app";
import type {
  OperationLogEntry,
  RegistryChangePreview,
  RegistryEntryRecord,
  RegistryIssue,
  RegistryRollbackRecord,
  RiskLevel,
} from "@/types";

const TEXT = {
  title: "注册表安全治理",
  subtitle:
    "仅处理可解释、可备份、可回滚的启动项和路径引用问题，不提供模糊批量清理。",
  startupEntries: "启动项列表",
  pathIssues: "路径引用异常",
  preview: "变更预览",
  dryRun: "仅做 dry-run",
  backup: "先备份",
  disable: "禁用启动项",
  rollback: "回滚本次修改",
  exists: "路径存在",
  missing: "路径缺失",
  safe: "可修改",
  protected: "受保护",
  noEntries: "当前没有读取到可展示的启动项。",
  noIssues: "当前没有检测到路径引用异常。",
  startupName: "名称",
  targetPath: "目标路径",
  risk: "风险",
  tags: "标签",
  action: "操作",
  beforeValue: "修改前",
  afterValue: "修改后",
  postChecks: "修改后验证",
};

const message = useMessage();
const store = useAppStore();
const { addHistory } = store;
const { getHistory } = useCleanup();
const {
  loading,
  error,
  listStartupEntries,
  listPathIssues,
  exportBackup,
  previewChange,
  applyChange,
  rollbackChange,
} = useRegistry();

const entries = ref<RegistryEntryRecord[]>([]);
const issues = ref<RegistryIssue[]>([]);
const dryRun = ref(true);
const selectedPreview = ref<RegistryChangePreview | null>(null);
const selectedBackupId = ref<string | null>(null);
const previewVisible = ref(false);
const lastRollback = ref<RegistryRollbackRecord | null>(null);

onMounted(async () => {
  await refresh();
});

async function refresh() {
  const [entryList, issueList] = await Promise.all([listStartupEntries(), listPathIssues()]);
  entries.value = entryList;
  issues.value = issueList;
}

function riskType(value: RiskLevel): "success" | "warning" | "error" {
  if (value === "low") return "success";
  if (value === "medium") return "warning";
  return "error";
}

async function handleBackup(entry: RegistryEntryRecord) {
  const backup = await exportBackup(entry.id);
  if (backup) {
    selectedBackupId.value = backup.backupId;
    message.success(`已创建备份：${backup.backupId}`);
  }
}

async function handlePreview(entry: RegistryEntryRecord) {
  const preview = await previewChange(entry.id);
  if (!preview) return;
  selectedPreview.value = preview;
  previewVisible.value = true;
}

async function handleApply() {
  if (!selectedPreview.value) return;
  const result = await applyChange(selectedPreview.value.entry.id, dryRun.value);
  if (!result) return;
  selectedPreview.value = result;
  selectedBackupId.value = result.backupId;
  const history = await getHistory();
  store.setHistory(history);
  message.success(dryRun.value ? "已完成 dry-run 预演" : "已完成启动项修改");
  await refresh();
}

async function handleRollback() {
  if (!selectedBackupId.value) return;
  const result = await rollbackChange(selectedBackupId.value);
  if (!result) return;
  lastRollback.value = result;
  const history = await getHistory();
  store.setHistory(history);
  message.success("已回滚本次注册表修改");
  await refresh();
}

const entryColumns: DataTableColumns<RegistryEntryRecord> = [
  {
    title: TEXT.startupName,
    key: "displayName",
    ellipsis: { tooltip: true },
  },
  {
    title: TEXT.targetPath,
    key: "targetPath",
    ellipsis: { tooltip: true },
    render: (row) => row.targetPath || row.valueData,
  },
  {
    title: TEXT.risk,
    key: "riskLevel",
    width: 100,
    render: (row) => h(NTag, { type: riskType(row.riskLevel), size: "small" }, () => row.riskLevel),
  },
  {
    title: TEXT.tags,
    key: "tags",
    render: (row) =>
      h(
        NSpace,
        { size: 6, wrap: true },
        () =>
          row.tags.map((tag) => h(NTag, { size: "small", round: true }, () => tag))
      ),
  },
  {
    title: TEXT.action,
    key: "action",
    width: 240,
    render: (row) =>
      h(NSpace, { size: 8 }, () => [
        h(
          NButton,
          { size: "small", secondary: true, onClick: () => void handleBackup(row) },
          () => TEXT.backup
        ),
        h(
          NButton,
          {
            size: "small",
            type: "primary",
            disabled: !row.safeToModify,
            onClick: () => void handlePreview(row),
          },
          () => TEXT.preview
        ),
      ]),
  },
];

const issueColumns: DataTableColumns<RegistryIssue> = [
  {
    title: "问题",
    key: "title",
  },
  {
    title: "摘要",
    key: "summary",
    ellipsis: { tooltip: true },
  },
  {
    title: TEXT.risk,
    key: "riskLevel",
    width: 100,
    render: (row) => h(NTag, { type: riskType(row.riskLevel), size: "small" }, () => row.riskLevel),
  },
];

const registryHistory = computed(() =>
  store.history.filter(
    (item: OperationLogEntry) =>
      item.recordKind === "registry_change" || item.recordKind === "registry_rollback"
  )
);
</script>

<template>
  <div class="page-shell registry-page">
    <section class="page-hero">
      <n-space vertical :size="18">
        <div>
          <div class="registry-hero-kicker">Registry Safety Desk</div>
          <div class="page-hero__title">{{ TEXT.title }}</div>
          <div class="page-hero__desc">{{ TEXT.subtitle }}</div>
        </div>

        <n-alert type="info" title="安全原则">
          仅支持可解释的单项治理：先备份、再预览、后执行、可回滚。
        </n-alert>
        <n-alert v-if="error" type="error" title="注册表操作失败">
          {{ error }}
        </n-alert>
      </n-space>
    </section>

    <n-card class="surface-card interactive-card">
      <template #header>
        <div class="section-head">
          <div>
            <div class="section-head__title">{{ TEXT.startupEntries }}</div>
            <div class="section-head__desc">优先展示当前用户级启动项，仅允许对低风险范围做显式修改。</div>
          </div>
        </div>
      </template>

      <n-data-table
        v-if="entries.length > 0"
        :columns="entryColumns"
        :data="entries"
        :loading="loading"
        size="small"
      />
      <n-empty v-else :description="TEXT.noEntries" />
    </n-card>

    <n-card class="surface-card interactive-card">
      <template #header>
        <div class="section-head">
          <div>
            <div class="section-head__title">{{ TEXT.pathIssues }}</div>
            <div class="section-head__desc">标记缺失路径和可疑命令行，帮助定位迁移后的引用问题。</div>
          </div>
        </div>
      </template>

      <n-data-table
        v-if="issues.length > 0"
        :columns="issueColumns"
        :data="issues"
        :loading="loading"
        size="small"
      />
      <n-empty v-else :description="TEXT.noIssues" />
    </n-card>

    <n-card class="surface-card interactive-card">
      <template #header>
        <div class="section-head">
          <div>
            <div class="section-head__title">注册表治理历史</div>
            <div class="section-head__desc">这里只展示注册表修改与回滚记录，便于复核和演示。</div>
          </div>
        </div>
      </template>

      <n-empty v-if="registryHistory.length === 0" description="当前还没有注册表治理历史。" />
      <n-list v-else bordered>
        <n-list-item v-for="item in registryHistory" :key="`${item.at}-${item.path}`">
          <div class="history-item">
            <div>
              <strong>{{ item.detail }}</strong>
              <div class="history-item__meta">{{ new Date(item.at).toLocaleString() }}</div>
            </div>
            <n-tag :type="item.success ? 'success' : 'error'" size="small">
              {{ item.recordKind }}
            </n-tag>
          </div>
        </n-list-item>
      </n-list>
    </n-card>

    <n-modal v-model:show="previewVisible" preset="card" style="width: 720px" :title="TEXT.preview">
      <n-space v-if="selectedPreview" vertical :size="16">
        <n-alert :type="riskType(selectedPreview.riskLevel)" title="风险说明">
          该修改默认只面向当前用户启动项，系统级启动项仍保持只读。
        </n-alert>

        <div class="filter-bar">
          <n-text>{{ TEXT.dryRun }}</n-text>
          <n-switch v-model:value="dryRun" />
        </div>

        <n-card size="small" embedded>
          <div class="kv-pair">
            <span>{{ TEXT.beforeValue }}</span>
            <code>{{ selectedPreview.beforeValue }}</code>
          </div>
          <div class="kv-pair">
            <span>{{ TEXT.afterValue }}</span>
            <code>{{ selectedPreview.afterValue }}</code>
          </div>
        </n-card>

        <n-card size="small" embedded>
          <template #header>{{ TEXT.postChecks }}</template>
          <n-list bordered>
            <n-list-item v-for="item in selectedPreview.postCheckItems" :key="item">
              {{ item }}
            </n-list-item>
          </n-list>
        </n-card>

        <n-space justify="end">
          <n-button @click="previewVisible = false">关闭</n-button>
          <n-button type="primary" @click="handleApply">{{ TEXT.disable }}</n-button>
          <n-button
            v-if="selectedBackupId"
            secondary
            type="warning"
            @click="handleRollback"
          >
            {{ TEXT.rollback }}
          </n-button>
        </n-space>
      </n-space>
    </n-modal>
  </div>
</template>

<style scoped>
.registry-page {
  display: flex;
  flex-direction: column;
  gap: 18px;
}

.history-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.history-item__meta {
  color: var(--text-tertiary, #74829a);
  font-size: 12px;
  margin-top: 4px;
}

.kv-pair {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.kv-pair code {
  white-space: pre-wrap;
  word-break: break-all;
}
</style>
