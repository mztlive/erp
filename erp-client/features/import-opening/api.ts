/**
 * W18 session-mock API：queryFn / mutationFn 纯函数。
 * 会话覆盖保存在本模块，避免与其它 wave 的 session-state 冲突。
 */

import { mockDelay } from "@/lib/mock-delay"
import type {
  FormalActionResponse,
  ImportBatchListQuery,
  ImportBatchListView,
  ImportBatchView,
  ImportConfirmationView,
  ImportIssuePage,
  ImportIssueQuery,
  ViewerRoleDemo,
} from "@/features/import-opening/types"
import {
  BATCH_STATUS_LABEL,
  CONFIRMATION_SCOPE_LABEL,
  OBJECT_CODE_LABEL,
} from "@/features/import-opening/types"
import {
  IMPORT_BATCH_SEEDS,
  IMPORT_ISSUE_SEEDS,
} from "@/mock/import-opening"

type BatchOverlay = {
  stage?: ImportBatchView["stage"]
  status?: ImportBatchView["status"]
  trialVersion?: string
  importRuleVersion?: string
  confirmations?: ImportConfirmationView[]
  invalidation?: ImportBatchView["invalidation"]
  backgroundJob?: ImportBatchView["backgroundJob"]
  formalDataFormed?: boolean
  notFormalDataMessage?: string
  version?: string
  updatedAt?: string
  actionBlockers?: ImportBatchView["actionBlockers"]
  productionGates?: ImportBatchView["productionGates"]
  allowedActions?: ImportBatchView["allowedActions"]
}

const batchOverlays = new Map<string, BatchOverlay>()
const repairLinks = new Map<string, { repairBatchId: string; repairBatchNo: string }>()

function nowIso() {
  return new Date().toISOString()
}

function projectBatch(
  seed: ImportBatchView,
  role: ViewerRoleDemo
): ImportBatchView {
  const overlay = batchOverlays.get(seed.batchId)
  const repair = repairLinks.get(seed.batchId)

  const confirmations = (
    overlay?.confirmations ?? seed.confirmations
  ).map((c) => {
    const inScope =
      role === "WAREHOUSE_CONFIRMER"
        ? c.scope === "WAREHOUSE"
        : role === "FINANCE_CONFIRMER"
          ? c.scope === "FINANCE"
          : false
    return {
      ...c,
      inViewerResponsibility: inScope,
    }
  })

  const productionGates = overlay?.productionGates ?? seed.productionGates
  const actionBlockers = [
    ...(overlay?.actionBlockers ?? seed.actionBlockers),
  ]

  // 系统管理员不能代替确认
  if (role === "SYSTEM_ADMIN") {
    if (!actionBlockers.some((b) => b.action === "CONFIRM_SCOPE" && b.code === "ADMIN_CANNOT_CONFIRM")) {
      actionBlockers.push({
        action: "CONFIRM_SCOPE",
        code: "ADMIN_CANNOT_CONFIRM",
        message: "系统管理员只负责技术编排，不能代替责任部门作业务确认。",
      })
    }
  }

  // work_item_type 未登记 — 始终 blocker（诚实 mock）
  if (!productionGates.workItemTypeRegistered) {
    if (
      !actionBlockers.some(
        (b) => b.code === "IMPORT_CONFIRM_WORK_ITEM_TYPE_NOT_REGISTERED"
      )
    ) {
      actionBlockers.push({
        action: "CONFIRM_SCOPE",
        code: "IMPORT_CONFIRM_WORK_ITEM_TYPE_NOT_REGISTERED",
        message:
          "导入确认任务类型尚未登记；业务确认入口暂不可用。",
      })
    }
  }

  return {
    ...seed,
    stage: overlay?.stage ?? seed.stage,
    status: overlay?.status ?? seed.status,
    trialVersion: overlay?.trialVersion ?? seed.trialVersion,
    importRuleVersion: overlay?.importRuleVersion ?? seed.importRuleVersion,
    confirmations,
    invalidation: overlay?.invalidation ?? seed.invalidation,
    backgroundJob: overlay?.backgroundJob ?? seed.backgroundJob,
    formalDataFormed: overlay?.formalDataFormed ?? seed.formalDataFormed,
    notFormalDataMessage:
      overlay?.notFormalDataMessage ?? seed.notFormalDataMessage,
    version: overlay?.version ?? seed.version,
    updatedAt: overlay?.updatedAt ?? seed.updatedAt,
    actionBlockers,
    productionGates,
    allowedActions: overlay?.allowedActions ?? seed.allowedActions,
    repairBatchId: repair?.repairBatchId ?? seed.repairBatchId,
    repairBatchNo: repair?.repairBatchNo ?? seed.repairBatchNo,
  }
}

function toListItem(batch: ImportBatchView) {
  const confirmed = batch.confirmations.filter((c) => c.result === "CONFIRMED")
    .length
  const total = batch.confirmations.length
  return {
    batchId: batch.batchId,
    batchNo: batch.batchNo,
    environment: batch.environment,
    sourceObjectSet: batch.sourceObjectSet,
    baselineDate: batch.baselineDate,
    importRuleVersion: batch.importRuleVersion,
    stage: batch.stage,
    status: batch.status,
    progressLabel: batch.backgroundJob
      ? `${batch.backgroundJob.processed}/${batch.backgroundJob.total}`
      : BATCH_STATUS_LABEL[batch.status],
    confirmationSummary:
      total === 0 ? "—" : `${confirmed}/${total} 已确认`,
    initiatorLabel: batch.initiatorLabel,
    updatedAt: batch.updatedAt,
  }
}

export async function fetchImportBatchList(
  query: ImportBatchListQuery & { role?: ViewerRoleDemo }
): Promise<ImportBatchListView> {
  await mockDelay()
  const role = query.role ?? "SYSTEM_ADMIN"
  let rows = IMPORT_BATCH_SEEDS.map((s) => projectBatch(s, role)).filter(
    (b) => b.environment === query.environment
  )

  if (query.status && query.status !== "all") {
    rows = rows.filter((b) => b.status === query.status)
  }
  if (query.objectType && query.objectType !== "all") {
    const objectType = query.objectType
    rows = rows.filter((b) => b.sourceObjectSet.includes(objectType))
  }
  if (query.q?.trim()) {
    const q = query.q.trim().toLowerCase()
    rows = rows.filter(
      (b) =>
        b.batchNo.toLowerCase().includes(q) ||
        b.batchId.toLowerCase().includes(q)
    )
  }

  const allForEnv = IMPORT_BATCH_SEEDS.map((s) => projectBatch(s, role)).filter(
    (b) => b.environment === query.environment
  )
  const metrics = {
    pendingValidate: allForEnv.filter((b) =>
      ["RECEIVING", "SCANNING", "VALIDATING", "TRIAL_READY"].includes(b.status)
    ).length,
    pendingConfirm: allForEnv.filter((b) =>
      ["AWAITING_CONFIRMATION", "CONFIRMATION_BLOCKED"].includes(b.status)
    ).length,
    applying: allForEnv.filter((b) => b.status === "APPLYING").length,
    failedOrPartial: allForEnv.filter((b) =>
      ["PARTIAL_SUCCESS", "FAILED"].includes(b.status)
    ).length,
  }

  const start = (query.page - 1) * query.pageSize
  const pageRows = rows.slice(start, start + query.pageSize)

  return {
    metrics,
    rows: pageRows.map(toListItem),
    totalCount: rows.length,
    queriedAt: nowIso(),
  }
}

export async function fetchImportBatchDetail(input: {
  batchId: string
  role?: ViewerRoleDemo
}): Promise<ImportBatchView | null> {
  await mockDelay()
  const role = input.role ?? "SYSTEM_ADMIN"
  const seed = IMPORT_BATCH_SEEDS.find((b) => b.batchId === input.batchId)
  if (!seed) return null
  return projectBatch(seed, role)
}

export async function fetchImportIssues(
  query: ImportIssueQuery
): Promise<ImportIssuePage> {
  await mockDelay()
  let rows = IMPORT_ISSUE_SEEDS.filter((i) => i.batchId === query.batchId)

  // 问题表只含失败/冲突/跳过/待映射 — 不混入成功长表
  if (query.issueCode && query.issueCode !== "all") {
    rows = rows.filter((i) => i.issueCode === query.issueCode)
  }
  if (query.objectType && query.objectType !== "all") {
    rows = rows.filter((i) => i.objectType === query.objectType)
  }
  if (query.rowStatus && query.rowStatus !== "all") {
    rows = rows.filter((i) => i.rowStatus === query.rowStatus)
  }

  rows = [...rows].sort((a, b) => a.sourceRowNo - b.sourceRowNo)
  const start = (query.page - 1) * query.pageSize
  const pageRows = rows.slice(start, start + query.pageSize)

  return {
    rows: pageRows,
    totalCount: rows.length,
    issueVersion: `issv-${query.batchId}-1`,
    queriedAt: nowIso(),
  }
}

/** 演示：规则变化使旧确认失效 */
export async function invalidateTrialByRuleChange(input: {
  batchId: string
  idempotencyKey: string
}): Promise<FormalActionResponse> {
  await mockDelay(120)
  const seed = IMPORT_BATCH_SEEDS.find((b) => b.batchId === input.batchId)
  if (!seed) {
    return { status: "failed", code: "NOT_FOUND", message: "批次不存在" }
  }

  const prevTrial = seed.trialVersion
  const prevRule = seed.importRuleVersion
  const nextTrial = `${prevTrial}-re`
  const nextRule = prevRule.endsWith(".x")
    ? prevRule
    : `${prevRule}.x`

  const confirmations = seed.confirmations.map((c) => ({
    ...c,
    result: "INVALIDATED" as const,
    comment: c.comment ?? "规则/试算变化后失效",
  }))

  batchOverlays.set(input.batchId, {
    ...batchOverlays.get(input.batchId),
    trialVersion: nextTrial,
    importRuleVersion: nextRule,
    confirmations,
    invalidation: {
      reason: `规则 ${prevRule} → ${nextRule}，试算 ${prevTrial} → ${nextTrial}`,
      previousTrialVersion: prevTrial,
      previousRuleVersion: prevRule,
      invalidatedAt: nowIso(),
    },
    productionGates: {
      ...seed.productionGates,
      allConfirmationsComplete: false,
      trialVersionMatches: false,
      workItemTypeRegistered: false,
    },
    actionBlockers: [
      {
        action: "START_APPLY",
        code: "STALE_CONFIRMATION",
        message: "试算或规则变化使旧确认失效，禁止按旧版本应用。",
      },
      {
        action: "CONFIRM_SCOPE",
        code: "IMPORT_CONFIRM_WORK_ITEM_TYPE_NOT_REGISTERED",
        message:
          "导入确认任务类型尚未登记；业务确认入口暂不可用。",
      },
    ],
    status: "AWAITING_CONFIRMATION",
    stage: "CONFIRM",
    formalDataFormed: false,
    notFormalDataMessage:
      "规则或试算已变化，旧确认全部失效。业务数据尚未形成，禁止按旧版本提交应用。",
    version: `bv-inv-${Date.now()}`,
    updatedAt: nowIso(),
  })

  return {
    status: "succeeded",
    message: "已模拟规则变化：旧确认失效，应用已阻断",
    batchId: input.batchId,
    reference: input.idempotencyKey,
  }
}

/** 演示：创建修复批次入口（会话内链接） */
export async function openRepairBatch(input: {
  batchId: string
  idempotencyKey: string
}): Promise<FormalActionResponse & { repairBatchId?: string }> {
  await mockDelay(100)
  const seed = IMPORT_BATCH_SEEDS.find((b) => b.batchId === input.batchId)
  if (!seed) {
    return { status: "failed", code: "NOT_FOUND", message: "批次不存在" }
  }
  if (seed.status !== "PARTIAL_SUCCESS" && !seed.repairBatchId) {
    const existing = repairLinks.get(input.batchId)
    if (!existing && seed.batchId !== "ib_prod_partial_004") {
      return {
        status: "blocked",
        code: "NOT_PARTIAL",
        message: "仅部分成功批次可发起修复批次",
      }
    }
  }

  const repairBatchId = seed.repairBatchId ?? "ib_prod_repair_005"
  const repairBatchNo = seed.repairBatchNo ?? "IMP-20260721-R01"
  repairLinks.set(input.batchId, { repairBatchId, repairBatchNo })

  return {
    status: "succeeded",
    message: `已定位修复批次 ${repairBatchNo}`,
    batchId: input.batchId,
    reference: input.idempotencyKey,
    repairBatchId,
  }
}

/** 上传后仍保持「尚未形成业务数据」提示（演示） */
export async function acknowledgeUploadReceived(input: {
  batchId: string
}): Promise<FormalActionResponse> {
  await mockDelay(80)
  const seed = IMPORT_BATCH_SEEDS.find((b) => b.batchId === input.batchId)
  if (!seed) {
    return { status: "failed", code: "NOT_FOUND", message: "批次不存在" }
  }
  batchOverlays.set(input.batchId, {
    ...batchOverlays.get(input.batchId),
    stage: "RECEIVE",
    status: "SCANNING",
    formalDataFormed: false,
    notFormalDataMessage:
      "文件已安全接收并进入扫描；上传成功 ≠ 导入完成。尚未形成业务数据。",
    updatedAt: nowIso(),
  })
  return {
    status: "succeeded",
    message: "已记录安全接收（尚未形成业务数据）",
    batchId: input.batchId,
    reference: `recv-${input.batchId}`,
  }
}

export function formatObjectSet(
  codes: readonly string[]
): string {
  return codes
    .map((c) => OBJECT_CODE_LABEL[c as keyof typeof OBJECT_CODE_LABEL] ?? c)
    .join("、")
}

export function formatConfirmationSummary(
  confirmations: readonly ImportConfirmationView[]
): string {
  return confirmations
    .map(
      (c) =>
        `${CONFIRMATION_SCOPE_LABEL[c.scope]}：${
          c.result === "CONFIRMED"
            ? "已确认"
            : c.result === "REJECTED"
              ? "已退回"
              : c.result === "INVALIDATED"
                ? "已失效"
                : "待确认"
        }`
    )
    .join("；")
}
