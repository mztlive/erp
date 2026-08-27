/**
 * W17 商城同步与映射 · 客户端契约。
 * 第一阶段：商城 → ERP 商业记录单向同步；无 ERP 回写商业修改入口。
 */

import type { WorkItemStatus } from "@/features/work-items"

export type MallSyncViewName =
    | "overview"
    | "jobs"
    | "snapshots"
    | "mapping"
    | "reconciliation"
    | "history"

export type OwnershipStage = "FIRST_PHASE_MALL_OWNED" | "ARCHIVED"

export type SyncDirection = "MALL_TO_ERP_COMMERCIAL_FACT" | "SEALED_HISTORY"

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
    jobType: "BASELINE" | "INCREMENTAL" | "SINGLE_ORDER" | "RECONCILIATION"
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
    mappingStatus: "PENDING_MAPPING" | "APPLIED" | "DIFF" | "UNCHANGED"
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

export type MappingTaskWorkItemView = {
    workItemId: string
    workItemType: "BUSINESS_EXCEPTION"
    businessObjectType: "MASTER_MAPPING_TASK"
    businessObjectId: string
    subjectVersion: string
    taskVersion: string
    status: WorkItemStatus
    statusLabel: string
    processingState: "READY" | "APPROVAL_BLOCKED"
    ownerUser?: { id: string; displayName: string }
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
    differenceType:
        | "MALL_MISSING"
        | "ERP_MISSING"
        | "STATUS"
        | "FINGERPRINT"
        | "DUPLICATE"
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
    emptyReason?:
        | "NO_SCOPE"
        | "NO_TASKS"
        | "FILTER_NO_RESULT"
        | "SEALED_HISTORY"
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
          salesOrderNo?: string
          salesOrderRevisionId: string
          receivableResultReference?: string
          message: string
      }
    | {
          status: "failed"
          code: string
          message: string
          operationId?: string
          reapplyOperationStatus?: "FAILED"
      }
    | {
          status: "unknown"
          reapplyOperationStatus: "UNKNOWN"
          operationId: string
          message: string
          idempotencyKey: string
      }

export type RequestSourceFixResult =
    | {
          status: "succeeded"
          mappingTaskId: string
          mappingTaskStatus: "PENDING"
          workItemStatus: "OPEN"
          taskVersion: string
          mappingEvidenceEntryId: string
          recordedAt: string
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
    ARCHIVED: "已封存 · ERP 主责",
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

export const MAPPING_TYPE_LABEL: Record<
    MappingTaskBase["mappingType"],
    string
> = {
    CUSTOMER: "客户映射",
    CONTRACT: "合同映射",
    SETTLEMENT_PARTY: "结算主体",
    VOUCHER_CATEGORY: "卡券类目",
    UNIQUE_LINE: "唯一明细",
    AMOUNT_FORMAT: "金额格式",
}

export const OWNER_ROLE_LABEL: Record<
    "SALES" | "OPERATIONS" | "FINANCE",
    string
> = {
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

export const SOURCE_FIX_REASON_OPTIONS = [
    { value: "SOURCE_FIELD_MISSING", label: "来源字段缺失" },
    { value: "SOURCE_FIELD_CONFLICT", label: "来源字段矛盾" },
    { value: "SOURCE_EVIDENCE_REQUIRED", label: "需补充来源证据" },
    { value: "OTHER", label: "其他" },
] as const

// ─── 来源系统契约（D01 source_registry） ─────────────────────────────────────
// 拆分到 lib/source-system-types.ts，此处再导出保持既有导入路径不变。

export {
    SOURCE_SYSTEM_STATUS_LABEL,
    SOURCE_SYSTEM_TYPE_LABEL,
    type SourceSystemItem,
    type SourceSystemListParams,
    type SourceSystemPage,
    type SourceSystemStatus,
    type SourceSystemType,
} from "@/features/mall-sync/lib/source-system-types"
