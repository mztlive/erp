/**
 * W24 销售单主责迁移批次 · 客户端契约类型
 * 对齐 docs/ui-workspaces/w24-ownership-migration.md §5/§8（session-mock 语义）。
 */

import type { ImportStageKey } from "@/components/business"

/** 批次向导六阶段（文档 §4.2） */
export type MigrationWizardStage =
  | "SCOPE"
  | "CONFIRMATIONS"
  | "FREEZE_SYNC"
  | "BASELINE"
  | "EXECUTION"
  | "COMPLETE"

export type BatchStatus =
  | "PREPARING"
  | "AWAITING_CONFIRMATION"
  | "FROZEN"
  | "BASELINE_CONFIRMED"
  | "EXECUTING"
  | "COMPLETED"
  | "FAILED"

export type ConfirmationState = "MISSING" | "VALID" | "INVALIDATED"

export type CheckRunStatus = "PASSED" | "BLOCKED" | "NOT_RUN"

export type ItemCheckStatus = "PASSED" | "BLOCKED" | "NOT_RUN" | "EXCLUDED"

export type ItemResultStatus =
  | "PENDING"
  | "ELIGIBLE"
  | "BLOCKED"
  | "EXCLUDED_DRAFT"
  | "EXCLUDED_VOIDED"
  | "MIGRATED"
  | "NOT_MIGRATED"

export type CutoverStatus = "PREPARING" | "ENABLED"

export type CutoverCheckStatus = "PASSED" | "FAILED"

export type FormalResultStatus =
  | "COMMITTED"
  | "NOT_COMMITTED"
  | "RUNNING"
  | "RESULT_UNKNOWN"

/** 演示角色（URL ?role=，不写入业务记录） */
export type ViewerRoleDemo =
  | "SYSTEM_ADMIN"
  | "CUTOVER_OWNER"
  | "SALES_CONFIRMER"
  | "FINANCE_CONFIRMER"
  | "BUSINESS_USER"
  | "NO_MODULE"

export type OverviewViewMode = "all" | "my_customers"

export type ConfirmationFilter =
  | "pending_sales"
  | "pending_finance"
  | "pending_baseline"
  | "invalidated"

export type BlockerCode =
  | "MAPPING"
  | "SINGLE_LINE"
  | "FINANCE"
  | "CARD_BASELINE"
  | "SYNC_WATERMARK"
  | "SCOPE_DRIFT"

export const WIZARD_TO_INDICATOR: Record<MigrationWizardStage, ImportStageKey> =
  {
    SCOPE: "upload",
    CONFIRMATIONS: "mapping",
    FREEZE_SYNC: "validation",
    BASELINE: "preview",
    EXECUTION: "submission",
    COMPLETE: "result",
  }

export const WIZARD_STAGE_LABEL: Record<MigrationWizardStage, string> = {
  SCOPE: "范围清单",
  CONFIRMATIONS: "双确认",
  FREEZE_SYNC: "冻结与最后同步",
  BASELINE: "最终基线",
  EXECUTION: "执行",
  COMPLETE: "完成",
}

export const WIZARD_ORDER: MigrationWizardStage[] = [
  "SCOPE",
  "CONFIRMATIONS",
  "FREEZE_SYNC",
  "BASELINE",
  "EXECUTION",
  "COMPLETE",
]

export const BATCH_STATUS_LABEL: Record<BatchStatus, string> = {
  PREPARING: "准备中",
  AWAITING_CONFIRMATION: "待确认",
  FROZEN: "已冻结",
  BASELINE_CONFIRMED: "基线已确认",
  EXECUTING: "执行中",
  COMPLETED: "已完成",
  FAILED: "执行失败·仍冻结",
}

export const BATCH_STATUS_TONE: Record<
  BatchStatus,
  "neutral" | "info" | "success" | "warning" | "destructive"
> = {
  PREPARING: "info",
  AWAITING_CONFIRMATION: "warning",
  FROZEN: "warning",
  BASELINE_CONFIRMED: "info",
  EXECUTING: "info",
  COMPLETED: "success",
  FAILED: "destructive",
}

export const CONFIRMATION_STATE_LABEL: Record<ConfirmationState, string> = {
  MISSING: "待确认",
  VALID: "已确认",
  INVALIDATED: "已失效",
}

export const CONFIRMATION_STATE_TONE: Record<
  ConfirmationState,
  "neutral" | "info" | "success" | "warning" | "destructive"
> = {
  MISSING: "warning",
  VALID: "success",
  INVALIDATED: "destructive",
}

export const BLOCKER_CODE_LABEL: Record<BlockerCode, string> = {
  MAPPING: "映射未完成",
  SINGLE_LINE: "非唯一卡券明细",
  FINANCE: "票款阻塞",
  CARD_BASELINE: "卡实例/余额基线",
  SYNC_WATERMARK: "同步进度",
  SCOPE_DRIFT: "范围摘要变化",
}

export const ITEM_STATUS_LABEL: Record<ItemResultStatus, string> = {
  PENDING: "待检查",
  ELIGIBLE: "可迁移（未提交）",
  BLOCKED: "阻塞",
  EXCLUDED_DRAFT: "商城草稿·不迁移",
  EXCLUDED_VOIDED: "已作废·不迁移",
  MIGRATED: "已迁移",
  NOT_MIGRATED: "未迁移",
}

export const ROLE_LABEL: Record<ViewerRoleDemo, string> = {
  SYSTEM_ADMIN: "系统管理员",
  CUTOVER_OWNER: "上线负责人",
  SALES_CONFIRMER: "销售确认人",
  FINANCE_CONFIRMER: "财务确认人",
  BUSINESS_USER: "业务用户（有限范围）",
  NO_MODULE: "无模块权限",
}

/** 固定切换检查码（文档 §5.4） */
export const CUTOVER_CHECK_CODES = [
  "PRODUCT_PUBLICATION",
  "SALES_PROJECTION",
  "MALL_FACT_INTAKE",
  "SUPPLIER_ORDER",
  "SUPPLIER_REJECTION",
  "AFTER_SALES_CANCEL",
  "MALL_REFUND",
  "CARD_BALANCE_RESTORATION",
  "SUPPLIER_REFUND",
  "COST_FINALIZATION",
  "SUPPLIER_SETTLEMENT",
  "PAYABLE_LINKAGE",
  "MANUAL_EXCEPTION",
  "RECONCILIATION",
  "BACKFILL_CAPABILITY",
  "PHASE1_POLLING_STOPPED",
  "MALL_B2B_ENTRY_CLOSED",
  "MALL_COMMERCIAL_FIELDS_READONLY",
] as const

export type CutoverCheckCode = (typeof CUTOVER_CHECK_CODES)[number]

export const CUTOVER_CHECK_LABEL: Record<CutoverCheckCode, string> = {
  PRODUCT_PUBLICATION: "商品发布",
  SALES_PROJECTION: "销售执行信息",
  MALL_FACT_INTAKE: "商城记录接入",
  SUPPLIER_ORDER: "供应商下单",
  SUPPLIER_REJECTION: "供应商拒单",
  AFTER_SALES_CANCEL: "售后取消",
  MALL_REFUND: "商城退款",
  CARD_BALANCE_RESTORATION: "卡余额恢复",
  SUPPLIER_REFUND: "供应商退款",
  COST_FINALIZATION: "成本定稿",
  SUPPLIER_SETTLEMENT: "供应商结算",
  PAYABLE_LINKAGE: "应付联动",
  MANUAL_EXCEPTION: "人工例外",
  RECONCILIATION: "对账",
  BACKFILL_CAPABILITY: "回填能力",
  PHASE1_POLLING_STOPPED: "一期轮询封存",
  MALL_B2B_ENTRY_CLOSED: "B2B 入口关闭",
  MALL_COMMERCIAL_FIELDS_READONLY: "商业字段只读",
}

export type ConfirmationSummary = {
  state: ConfirmationState
  confirmedBy?: string
  confirmedAt?: string
  subjectHash?: string
  invalidatedReason?: string
  /** 失效前审计（保留展示） */
  priorAudit?: {
    confirmedBy: string
    confirmedAt: string
    subjectHash: string
  }
}

export type ActionBlocker = {
  action: string
  code: string
  message: string
}

export type OwnershipMigrationBatchRow = {
  batchId: string
  batchNo: string
  sourceMallId: string
  sourceMallName: string
  customerId: string
  customerName: string
  /** 一批仅一位客户 — 不得出现多客户文案 */
  singleCustomer: true
  scopeHash: string
  status: BatchStatus
  freezeActive: boolean
  eligibleCount: number
  blockedCount: number
  /** 仅 COMPLETED 后才可等于项目数；执行中/失败不得展示暂存成功数 */
  migratedCount: number
  salesConfirmation: ConfirmationSummary
  financeConfirmation: ConfirmationSummary
  baselineConfirmation: ConfirmationSummary
  lastSyncWatermark?: string
  errorSummary?: string
  primaryBlocker?: BlockerCode
  primaryBlockerLabel?: string
  allowedActions: string[]
  actionBlockers: ActionBlocker[]
  updatedAt: string
}

export type OwnershipMigrationListQuery = {
  mallId: string
  customerId?: string
  status?: BatchStatus | "open"
  confirmation?: ConfirmationFilter
  blocker?: BlockerCode
  view?: OverviewViewMode
  q?: string
  page: number
  pageSize: number
  role?: ViewerRoleDemo
}

export type OverviewMetrics = {
  pendingPrepare: number
  pendingSales: number
  pendingFinance: number
  pendingBaseline: number
  executable: number
  failedFrozen: number
  completed: number
}

export type OverviewStatusSummary = {
  phase1WatermarkLabel: string
  freezeActive: boolean
  freezeStartedAt?: string
  freezeScopeLabel: string
  migratedCustomers: number
  totalCustomers: number
  migratedOrders: number
  totalOrders: number
  tStatus: "NOT_REGISTERED" | "ENABLED"
  tEnabledAt?: string
}

export type OwnershipMigrationListView = {
  hasModuleAccess: boolean
  hasCustomerScope: boolean
  mallId: string
  mallName: string
  metrics: OverviewMetrics
  statusSummary: OverviewStatusSummary
  rows: OwnershipMigrationBatchRow[]
  totalCount: number
  queriedAt: string
  /** 字段掩码：票款金额等 */
  financeFieldsMasked: boolean
}

export type MigrationCheck = {
  code: string
  status: CheckRunStatus
  subjectHash: string
  summary: string
  destinationWorkspaceId?: string
  objectId?: string
}

export type MigrationItem = {
  itemId: string
  salesOrderId: string
  salesOrderNo: string
  sourceIdentityId: string
  sourceOrderNo: string
  salesOrderStatus: string
  beforeOwnerSystem: "MALL"
  afterOwnerSystem: "ERP"
  baselineSalesOrderRevisionId?: string
  /** 第一份执行投影修订，不是新销售版本 */
  baselineProjectionRevisionId?: string
  checkResults: Record<string, ItemCheckStatus>
  itemStatus: ItemResultStatus
  errorSummary?: string
  /** 排除说明：草稿/作废不入统计 */
  exclusionReason?: string
}

export type ScopeExclusion = {
  kind: "MALL_DRAFT" | "VOIDED"
  count: number
  reason: string
}

export type OwnershipMigrationBatchView = {
  identity: {
    batchId: string
    batchNo: string
    sourceMallId: string
    sourceMallName: string
    customerId: string
    customerName: string
  }
  status: BatchStatus
  stage: MigrationWizardStage
  scopeHash: string
  freeze: {
    active: boolean
    startedAt?: string
    scopeLabel: string
  }
  counts: {
    total: number
    eligible: number
    blocked: number
    migrated: number
  }
  exclusions: ScopeExclusion[]
  salesVersionSummary: string
  financeSummary: string
  financeSummaryMasked: boolean
  cardBaselineSummary: string
  lastSyncWatermark?: string
  fullReconcileDone: boolean
  confirmations: {
    sales: ConfirmationSummary
    finance: ConfirmationSummary
    baseline: ConfirmationSummary & { lastSyncWatermark?: string }
  }
  checks: MigrationCheck[]
  items: MigrationItem[]
  backgroundOperation?: {
    operationId: string
    status: "queued" | "running" | "succeeded" | "failed" | "frozen"
    /** 仅后台进度标签，不表达成功数 */
    progressLabel: string
    progressPercent: number
    startedAt?: string
    lastProgressAt?: string
  }
  formalResult?: {
    status: FormalResultStatus
    title: string
    description: string
    operationId?: string
    committedAt?: string
  }
  successSemanticsNote: string
  allowedActions: string[]
  actionBlockers: ActionBlocker[]
  objectVersion: string
  queriedAt: string
  viewerRole: ViewerRoleDemo
}

export type CutoverCheck = {
  checkCode: CutoverCheckCode
  checkNo: number
  checkStatus: CutoverCheckStatus
  subjectHash: string
  evidenceReference: string
  supersedesCheckId?: string
  checkedBy: string
  checkedAt: string
  /** 仅链尾代表当前证据 */
  isCurrentTail: boolean
  label: string
}

export type ConsumptionCutoverView = {
  cutoverId: string
  mallId: string
  mallName: string
  status: CutoverStatus
  migrationScopeDigest: string
  coveredBatchCount: number
  totalTargetBatchCount: number
  coveredSalesOrderCount: number
  allBatchesCompleted: boolean
  phase1PollingSealed: boolean
  prerequisites: Array<{
    key: string
    label: string
    passed: boolean
    detail: string
  }>
  enabledAt?: string
  enabledBy?: string
  confirmationDigest?: string
  checks: CutoverCheck[]
  /** 历史/被替代检查（不可当当前通过） */
  supersededChecks: CutoverCheck[]
  allowedActions: string[]
  actionBlockers: ActionBlocker[]
  objectVersion: string
  lastFormalResult?: {
    status: FormalResultStatus
    operationId: string
    message: string
  }
  queriedAt: string
}

export type MaintenanceFreezeView = {
  active: boolean
  startedAt?: string
  sourceMallName: string
  scopeLabel: string
  stageLabel: string
  responsibleRole: string
  frozenActions: string[]
  progressHref: string
  queriedAt: string
}

export type MigrationFormalCommand = {
  batchId?: string
  cutoverId?: string
  action:
    | "CREATE_BATCH"
    | "RECHECK_SCOPE"
    | "CONFIRM_SALES"
    | "CONFIRM_FINANCE"
    | "START_FREEZE"
    | "RUN_FINAL_SYNC"
    | "CONFIRM_BASELINE"
    | "EXECUTE_BATCH"
    | "RESUME_BATCH"
    | "ENABLE_CUTOVER"
    | "QUERY_FORMAL_RESULT"
    | "DEMO_INVALIDATE_SCOPE"
  expectedObjectVersion?: string
  expectedScopeHash?: string
  expectedSubjectHash?: string
  requestId: string
  customerId?: string
  role?: ViewerRoleDemo
}

export type MigrationFormalResult = {
  operationId: string
  batchId?: string
  cutoverId?: string
  status: FormalResultStatus
  batchStatus?: BatchStatus
  enabledAt?: string
  committedAt?: string
  nextAction: string
  message: string
}
