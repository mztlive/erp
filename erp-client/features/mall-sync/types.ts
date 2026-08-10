/**
 * W17 商城同步与映射 · 客户端契约（对齐 docs/ui-workspaces/w17-mall-sync-mapping.md §5/§8）。
 * 第一阶段：商城 → ERP 商业记录单向同步；无 ERP 回写商业修改入口。
 */

export type MallSyncViewName =
  | "overview"
  | "jobs"
  | "snapshots"
  | "mapping"
  | "reconciliation"
  | "history"

export type OwnershipStage =
  | "FIRST_PHASE_MALL_OWNED"
  | "SECOND_PHASE_ERP_OWNED"

export type SyncDirection =
  | "MALL_TO_ERP_COMMERCIAL_FACT"
  | "SEALED_HISTORY"

type ManualGovernancePolicy =
  | {
      state: "MISSING"
      blockerCode: "MANUAL_GOVERNANCE_POLICY_MISSING"
    }
  | {
      state: "CONFIGURED"
      policyVersion: string
      executionMode: "SINGLE_OPERATOR_REASON" | "DUAL_CONTROL_AUTHORIZATION"
    }

export type MallSyncMetric = {
  key: string
  label: string
  count?: number
  value?: string
  detail?: string
  visible: boolean
  targetView: MallSyncViewName
  targetFilter?: Record<string, string>
}

type MallSyncOwnership = {
  businessType: "VOUCHER"
  stage: OwnershipStage
  /** 同步范围内销售单的创建来源汇总（MALL/MIXED/ERP） */
  originSystemSummary: "MALL" | "MIXED" | "ERP"
  mallOwnedOrderCount?: number
  erpOwnedOrderCount?: number
  syncDirection: SyncDirection
  firstPhasePollingEnabled: boolean
  sealedAt?: string
  finalWatermark?: string
  /** 商城可写边界文案 */
  mallWriteBoundary: string
  /** ERP 可写边界文案 */
  erpWriteBoundary: string
}

type MallSyncContext = {
  sourceSystem: {
    id: string
    code: string
    name: string
    environmentLabel: string
  }
  manualGovernancePolicy: ManualGovernancePolicy
  ownership: MallSyncOwnership
  freshness: {
    currentWatermark?: string
    latestSuccessfulJobAt?: string
    sourceSafeTime?: string
    syncLagSeconds?: number
    viewProjectedAt: string
  }
  metrics: MallSyncMetric[]
  /** §9 来源商城不可用 */
  sourceUnavailable?: boolean
  sourceUnavailableMessage?: string
  hasSourceScope: boolean
  /** 定时增量说明（策略缺失时仍可读） */
  scheduledIncrementalNote: string
}

export type MallSyncJobRow = {
  jobId: string
  jobNo: string
  jobType:
    | "BASELINE"
    | "INCREMENTAL"
    | "SINGLE_ORDER"
    | "RECONCILIATION"
  jobTypeLabel: string
  rangeStart?: string
  rangeEnd?: string
  status: "RUNNING" | "SUCCEEDED" | "PARTIAL_FAILED" | "FAILED"
  statusLabel: string
  statusTone: "info" | "success" | "warning" | "destructive" | "neutral"
  pageCount: number
  itemCount: number
  errorCount: number
  cursorBefore?: string
  cursorAfter?: string
  startedAt: string
  finishedAt?: string
  errorClass?: string
  impactSummary?: string
  triggeredBy: string
  watermarkAdvanced: boolean
  allowedActions: string[]
  actionBlockers: Array<{ action: string; code: string; message: string }>
}

export type MallSnapshotRow = {
  snapshotId: string
  externalOrderNo: string
  sourceUpdatedAt: string
  observedAt: string
  sourceStatusCode: string
  sourceStatusLabel: string
  contentHashShort: string
  /** 快照处理状态（与映射任务/重新归集独立） */
  mappingStatus:
    | "PENDING_MAPPING"
    | "APPLIED"
    | "DIFF"
    | "UNCHANGED"
  mappingStatusLabel: string
  appliedSalesOrderId?: string
  appliedSalesOrderNo?: string
  appliedRevisionId?: string
  syncJobId: string
  syncJobNo: string
  conflictFlags: string[]
  /** 白名单商业字段（禁止玩法/卡号/卡密/绑定手机/密钥） */
  whitelistFields: Array<{ field: string; label: string; value: string }>
}

type MappingTaskWorkItemView = {
  workItemId: string
  workItemType: "BUSINESS_EXCEPTION"
  businessObjectType: "MASTER_MAPPING_TASK"
  businessObjectId: string
  subjectVersion: string
  subjectHash: string
  status: "UNCLAIMED" | "PENDING" | "IN_PROGRESS" | "COMPLETED" | "TRANSFERRED" | "CLOSED"
  statusLabel: string
  completionAction: string
  claimedBy?: string
}

type MappingCandidate = {
  objectType: string
  objectId: string
  stableNo: string
  label: string
  currentRevisionId: string
  eligibility: "ELIGIBLE" | "INELIGIBLE"
  reason: string
}

type MappingCurrentTarget = {
  objectType: string
  objectId: string
  stableNo: string
  label: string
  relationRole: string
  validFrom: string
  validTo?: string
  status: string
}

export type MappingTaskBase = {
  mappingTaskId: string
  sourceSnapshotId: string
  externalOrderNo: string
  externalIdentityMapId?: string
  mappingType:
    | "CUSTOMER"
    | "CONTRACT"
    | "SETTLEMENT_PARTY"
    | "VOUCHER_CATEGORY"
    | "UNIQUE_LINE"
    | "AMOUNT_FORMAT"
  mappingTypeLabel: string
  mappingTaskStatus: "PENDING" | "RESOLVED" | "UNRESOLVABLE" | "CLOSED"
  mappingTaskStatusLabel: string
  reapplyOperation?: {
    operationId: string
    status: "QUEUED" | "RUNNING" | "SUCCEEDED" | "FAILED" | "UNKNOWN"
    statusLabel: string
    lastUpdatedAt: string
    salesOrderId?: string
    salesOrderNo?: string
    salesOrderRevisionId?: string
    receivableResultReference?: string
  }
  sourceEvidence: Array<{
    field: string
    label: string
    value: string
    sensitive?: boolean
  }>
  candidateTargets: MappingCandidate[]
  currentTargets: MappingCurrentTarget[]
  impactSummary: string
  resolutionHistory: Array<{
    action: string
    result: string
    handledBy: string
    handledAt: string
    evidenceReference?: string
  }>
  allowedActions: string[]
  actionBlockers: Array<{ action: string; code: string; message: string }>
  lockVersion: number
  /** 冲突：谱系与候选不一致 */
  hasConflict?: boolean
}

export type MappingTaskView =
  | (MappingTaskBase & {
      ownerRoutingState: "MISSING"
      ownerRole?: never
      ownerUserId?: never
      workItem?: never
    })
  | (MappingTaskBase & {
      ownerRoutingState: "CONFIGURED"
      ownerRole: "SALES" | "OPERATIONS" | "FINANCE"
      ownerRoleLabel: string
      ownerUserId?: string
      workItem: MappingTaskWorkItemView
    })

export type ReconciliationDifference = {
  differenceId: string
  externalOrderNo: string
  differenceType: "MALL_MISSING" | "ERP_MISSING" | "STATUS" | "FINGERPRINT" | "DUPLICATE"
  differenceTypeLabel: string
  sourceFingerprintShort?: string
  erpSalesOrderNo?: string
  erpFingerprintShort?: string
  status: "OPEN" | "PULLING" | "RESOLVED" | "CONFIRMED"
  statusLabel: string
  statusTone: "warning" | "info" | "success" | "destructive" | "neutral"
  impactSummary: string
}

export type ReconciliationBatch = {
  jobId: string
  jobNo: string
  boundaryLabel: string
  mallCount: number
  erpCount: number
  differenceCount: number
  status: "RUNNING" | "SUCCEEDED" | "DIFFERENCE" | "FAILED"
  statusLabel: string
  startedAt: string
  finishedAt?: string
  differences: ReconciliationDifference[]
}

type HistoryArchiveEntry = {
  id: string
  kind: "SEAL" | "MAPPING"
  title: string
  summary: string
  recordedAt: string
  watermark?: string
  reference?: string
}

export type MallSyncPageView = {
  context: MallSyncContext
  jobs: MallSyncJobRow[]
  snapshots: MallSnapshotRow[]
  mappingTasks: MappingTaskView[]
  reconciliation: ReconciliationBatch | null
  history: HistoryArchiveEntry[]
  selectedJob?: MallSyncJobRow
  selectedSnapshot?: MallSnapshotRow
  selectedMappingTask?: MappingTaskView
  selectedDifference?: ReconciliationDifference
  emptyReason?: "NO_SCOPE" | "NO_TASKS" | "FILTER_NO_RESULT" | "SEALED_HISTORY"
}

export type TriggerMallSyncResult =
  | {
      status: "succeeded"
      jobId: string
      jobNo: string
      message: string
    }
  | {
      status: "failed"
      code: string
      message: string
    }
  | {
      status: "unknown"
      message: string
      idempotencyKey: string
    }

export type ConfirmMappingResult =
  | {
      status: "succeeded"
      mappingTaskId: string
      mappingTaskStatus: "RESOLVED"
      externalIdentityMapId: string
      mappingTargetId: string
      recordedAt: string
      message: string
    }
  | {
      status: "failed"
      code: string
      message: string
    }
  | {
      status: "unknown"
      message: string
      idempotencyKey: string
    }

export type ReapplyResult =
  | {
      status: "succeeded"
      operationId: string
      reapplyOperationStatus: "SUCCEEDED"
      salesOrderId: string
      salesOrderNo: string
      salesOrderRevisionId: string
      receivableResultReference?: string
      message: string
    }
  | {
      status: "failed"
      code: string
      message: string
    }
  | {
      status: "unknown"
      reapplyOperationStatus: "UNKNOWN"
      operationId: string
      message: string
      idempotencyKey: string
    }

export type DeferMappingResult =
  | {
      status: "succeeded"
      mappingTaskId: string
      mappingTaskStatus: "PENDING"
      leaseDisposition: "RETAINED" | "RELEASED"
      nextQueueCursor?: string
      message: string
    }
  | {
      status: "failed"
      code: string
      message: string
    }

export const VIEW_LABEL: Record<MallSyncViewName, string> = {
  overview: "运行总览",
  jobs: "同步任务",
  snapshots: "来源数据",
  mapping: "映射任务",
  reconciliation: "每日核对",
  history: "历史",
}

export const STAGE_LABEL: Record<OwnershipStage, string> = {
  FIRST_PHASE_MALL_OWNED: "第一阶段 · 商城开单",
  SECOND_PHASE_ERP_OWNED: "已封存 · ERP 主责",
}

export const DIRECTION_LABEL: Record<SyncDirection, string> = {
  MALL_TO_ERP_COMMERCIAL_FACT: "商城 → ERP 商业记录",
  SEALED_HISTORY: "历史只读",
}

/** 同步任务失败分类中文映射（errorClass） */
export const JOB_ERROR_CLASS_LABEL: Record<string, string> = {
  SOURCE_PAGE_TIMEOUT: "来源分页读取超时",
  SOURCE_ORDER_NOT_FOUND: "来源单号未命中",
}

export const MAPPING_TYPE_LABEL: Record<MappingTaskBase["mappingType"], string> = {
  CUSTOMER: "客户映射",
  CONTRACT: "合同映射",
  SETTLEMENT_PARTY: "结算主体",
  VOUCHER_CATEGORY: "卡券类目",
  UNIQUE_LINE: "唯一明细",
  AMOUNT_FORMAT: "金额格式",
}

export const OWNER_ROLE_LABEL: Record<"SALES" | "OPERATIONS" | "FINANCE", string> =
  {
    SALES: "销售",
    OPERATIONS: "运营",
    FINANCE: "财务",
  }

export const JOB_TYPE_LABEL: Record<MallSyncJobRow["jobType"], string> = {
  BASELINE: "期初基线",
  INCREMENTAL: "增量拉取",
  SINGLE_ORDER: "单号补拉",
  RECONCILIATION: "每日核对",
}

export const DEFER_REASON_OPTIONS = [
  { value: "WAITING_SOURCE", label: "等待来源修复" },
  { value: "NEED_CLARIFICATION", label: "需业务澄清" },
  { value: "WAITING_MASTER_DATA", label: "等待基础资料就绪" },
  { value: "OTHER", label: "其他" },
] as const

// ─── P0-5 垂直样板：来源系统真实接口（D01 source_registry） ───────────────────

/**
 * 来源系统类型。锁定契约：system_type ∈ ERP | MALL | SUPPLIER（serde snake_case）。
 * 展示必须走 SOURCE_SYSTEM_TYPE_LABEL 中文映射，禁止枚举原值上屏（AGENTS.md §5）。
 */
export type SourceSystemType = "ERP" | "MALL" | "SUPPLIER"

/**
 * 来源系统状态。锁定契约：status ∈ 启用 | 停用（后端直接下发中文状态文案）。
 * 若后端实际产物与此不符（如改为 ACTIVE/INACTIVE 枚举码），需同步调整
 * SOURCE_SYSTEM_STATUS_LABEL 与 api.ts 的契约注释。
 */
export type SourceSystemStatus = "启用" | "停用"

/** 来源系统列表行（GET /admin/source-systems 的 items 项）。 */
export type SourceSystemItem = {
  id: string
  code: string
  name: string
  system_type: SourceSystemType
  status: SourceSystemStatus
  /** 创建时间：api-contract.md §5.1 约定秒级 Unix 时间戳；后端若返回 ISO 字符串则原样透传 */
  created_at: number | string
}

/** 来源系统分页查询参数（page 从 1 起，对齐 lib/api/paging.ts 的 PageParams）。 */
export type SourceSystemListParams = {
  page: number
  page_size: number
}

/** 来源系统分页响应（对齐 api-contract.md §3 分页形状）。 */
export type SourceSystemPage = {
  items: SourceSystemItem[]
  total: number
  page: number
  page_size: number
}

/** 来源系统类型中文映射（AGENTS.md §5：新增枚举必须同时写中文映射表）。 */
export const SOURCE_SYSTEM_TYPE_LABEL: Record<SourceSystemType, string> = {
  ERP: "ERP",
  MALL: "商城",
  SUPPLIER: "供应商",
}

/** 来源系统状态中文映射（契约值为中文；映射表为未知值兜底保留）。 */
export const SOURCE_SYSTEM_STATUS_LABEL: Record<SourceSystemStatus, string> = {
  启用: "启用",
  停用: "停用",
}
