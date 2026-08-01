/**
 * W07 session-mock API：queryFn / mutationFn 纯函数。
 * claimToken 仅出现在领取/续租响应，不进入队列查询 View。
 * 租约 / 完成 / 暂挂复用 W02 session-state 统一信封语义。
 */

import { mockDelay } from "@/features/workspace-kit/delay"
import type {
  ConfirmationLineDraft,
  CoverageByLine,
  FormalActionResponse,
  FormalOutcome,
  ProcurementConfirmationTask,
  ProcurementQueueView,
  RejectReasonCode,
  WorkItemLease,
} from "@/features/procurement-confirmation/types"
import { PROCUREMENT_CONFIRMATION_SEED } from "@/mock/procurement-confirmation"
import {
  applyWorkItemActionSession,
  claimWorkItemSession,
  clearSessionLease,
  completeWorkItemSession,
  finalizePendingComplete,
  getIdempotencyEntry,
  getProcurementBusinessOutcome,
  getProcurementDraft,
  getSessionLease,
  getSessionLeaseState,
  isProcurementWorkItemHeld,
  isProcurementWorkItemTerminal,
  markQueueTaskCompleted,
  markQueueTaskHeld,
  queryIdempotencyResult,
  saveProcurementDraft,
  setIdempotencySucceeded,
  setProcurementBusinessOutcome,
  WorkItemMockError,
} from "@/mock/session-state"

export type QueueFilters = {
  scope: "mine" | "role_pool"
  due?: "active" | "today" | "overdue"
  sort?: "due_at" | "submitted_at" | "priority"
  orderNo?: string
  currentWorkItemId?: string
  queueContextId?: string
}

const QUEUE_CLOCK = new Date("2026-08-01T10:00:00+08:00")
const QUEUE_DATE = "2026-08-01"

function recomputeCoverage(
  task: ProcurementConfirmationTask,
  lines: readonly ConfirmationLineDraft[]
): {
  coverageByLine: CoverageByLine[]
  estimatedPurchaseGross: string
  estimatedMargin?: string
  blockingIssues: ProcurementConfirmationTask["decisionSummary"]["blockingIssues"]
  warnings: ProcurementConfirmationTask["decisionSummary"]["warnings"]
} {
  const coverageByLine = task.salesSubmission.lines.map((line) => {
    const confirmed = lines
      .filter((c) => c.submissionLineId === line.submissionLineId)
      .reduce((sum, c) => sum + Number(c.confirmedQuantity || 0), 0)
    const required = Number(line.committedQuantity)
    const complete = confirmed + 1e-9 >= required && required > 0
    const gap = Math.max(0, required - confirmed)
    return {
      submissionLineId: line.submissionLineId,
      itemName: line.itemName,
      confirmed: confirmed.toFixed(0),
      required: line.committedQuantity,
      complete,
      gap: gap.toFixed(0),
    }
  })
  const incomplete = coverageByLine.filter((c) => !c.complete)
  const invalidQual = lines.filter((l) => l.qualificationStatus === "INVALID")
  const blockingIssues = [
    ...incomplete.map((c) => ({
      code: "QTY_COVERAGE_INCOMPLETE",
      message: `「${c.itemName}」已确认 ${c.confirmed}/${c.required}，缺口 ${c.gap}`,
      lineId: c.submissionLineId,
    })),
    ...invalidQual.map((l) => ({
      code: "QUALIFICATION_INVALID",
      message: `供应商「${l.supplierName}」资质失效，不得通过`,
      lineId: l.submissionLineId,
    })),
  ]
  const lateLines = lines.filter((cl) => {
    const sub = task.salesSubmission.lines.find(
      (s) => s.submissionLineId === cl.submissionLineId
    )
    return sub && cl.expectedDeliveryDate > sub.requestedDeliveryDate
  })
  const warnings = lateLines.map((l) => ({
    code: "DELIVERY_LATER_THAN_COMMITMENT",
    message: `「${l.supplierName}」预计交期 ${l.expectedDeliveryDate} 晚于客户期望`,
    lineId: l.submissionLineId,
  }))
  const purchase = lines
    .reduce(
      (sum, line) =>
        sum +
        Number(line.confirmedQuantity || 0) * Number(line.latestCostGross || 0),
      0
    )
    .toFixed(2)
  const sales = Number(task.salesSubmission.grossAmount)
  const estimatedMargin =
    sales > 0
      ? (((sales - Number(purchase)) / sales) * 100).toFixed(2) + "%"
      : undefined
  return {
    coverageByLine,
    estimatedPurchaseGross: purchase,
    estimatedMargin,
    blockingIssues,
    warnings,
  }
}

function projectTask(
  seed: ProcurementConfirmationTask
): ProcurementConfirmationTask | null {
  if (isProcurementWorkItemTerminal(seed.workItemId)) return null
  const draft = getProcurementDraft(seed.workItemId)
  const held = isProcurementWorkItemHeld(seed.workItemId)
  const publicLease = getSessionLeaseState(seed.workItemId)
  const lines = (draft?.lines as ConfirmationLineDraft[] | undefined) ??
    seed.confirmation.lines
  const editVersion = draft?.editVersion ?? seed.confirmation.editVersion
  const decision = recomputeCoverage(seed, lines)

  return {
    ...seed,
    status: held ? "IN_PROGRESS" : seed.status,
    held,
    // 查询 View：仅公开租约元数据，永不包含 claimToken
    lease: publicLease
      ? {
          claimedByLabel: "当前用户 · 李采购",
          expiresAt: publicLease.leaseExpiresAt,
          leaseVersion: publicLease.leaseVersion,
        }
      : seed.lease,
    confirmation: {
      ...seed.confirmation,
      editVersion,
      lines,
    },
    decisionSummary: {
      ...seed.decisionSummary,
      ...decision,
    },
    actionBlockers:
      decision.blockingIssues.length > 0
        ? [
            {
              action: "APPROVE",
              code: "QTY_COVERAGE_INCOMPLETE",
              message: "存在未完整覆盖的销售明细，服务端将拒绝通过",
            },
          ]
        : [],
    riskLabel: held ? "已暂挂" : seed.riskLabel,
    riskTone: held ? "warning" : seed.riskTone,
  }
}

function filterSummary(filters: QueueFilters): string {
  const parts = [
    filters.scope === "mine" ? "仅我的" : "角色池",
    filters.due === "overdue"
      ? "已超期"
      : filters.due === "today"
        ? "今日到期"
        : "有效全部",
    filters.sort === "priority"
      ? "优先级"
      : filters.sort === "submitted_at"
        ? "提交时间"
        : "截止优先",
  ]
  if (filters.orderNo) parts.push(`单号 ${filters.orderNo}`)
  return parts.join(" · ")
}

function sortTasks(
  tasks: ProcurementConfirmationTask[],
  sort: QueueFilters["sort"]
): ProcurementConfirmationTask[] {
  const copy = [...tasks]
  copy.sort((a, b) => {
    if (sort === "priority") return b.priority - a.priority
    if (sort === "submitted_at") {
      return a.salesSubmission.submittedAt.localeCompare(
        b.salesSubmission.submittedAt
      )
    }
    return a.dueAt.localeCompare(b.dueAt)
  })
  return copy
}

export async function fetchProcurementQueue(
  filters: QueueFilters
): Promise<ProcurementQueueView> {
  await mockDelay()
  const projected = PROCUREMENT_CONFIRMATION_SEED.map(projectTask).filter(
    (t): t is ProcurementConfirmationTask => t != null
  )

  let tasks = projected
  tasks = tasks.filter((task) => task.responsibilityScope === filters.scope)
  if (filters.orderNo) {
    const q = filters.orderNo.trim().toUpperCase()
    tasks = tasks.filter((t) =>
      t.salesSubmission.salesOrderNo.toUpperCase().startsWith(q)
    )
  }
  if (filters.due === "overdue") {
    tasks = tasks.filter((task) => new Date(task.dueAt) < QUEUE_CLOCK)
  } else if (filters.due === "today") {
    tasks = tasks.filter((task) => task.dueAt.slice(0, 10) === QUEUE_DATE)
  }

  tasks = sortTasks(tasks, filters.sort ?? "due_at")

  const queueContextId =
    filters.queueContextId ??
    `queue:procurement-confirmation:demo:${filters.scope}`

  let position = 0
  let current = tasks[0]
  if (filters.currentWorkItemId) {
    const idx = tasks.findIndex(
      (t) => t.workItemId === filters.currentWorkItemId
    )
    if (idx >= 0) {
      position = idx
      current = tasks[idx]
    }
  }

  const emptyReason =
    PROCUREMENT_CONFIRMATION_SEED.every((t) =>
      isProcurementWorkItemTerminal(t.workItemId)
    )
      ? "NO_TASKS"
      : tasks.length === 0
        ? "FILTER_NO_RESULT"
        : undefined

  return {
    preferences: {
      autoNextDefault: true,
      // preferenceScope 未配置 → 前端不得持久化
    },
    context: {
      queueContextId,
      position: tasks.length === 0 ? 0 : position + 1,
      total: tasks.length,
      currentWorkItemId: current?.workItemId,
      previousWorkItemId: tasks[position - 1]?.workItemId,
      nextWorkItemId: tasks[position + 1]?.workItemId,
      filterSummary: filterSummary(filters),
      queueContextUpdatedAt: new Date().toISOString(),
    },
    tasks,
    current,
    emptyReason,
  }
}

export async function claimProcurementWorkItem(
  workItemId: string
): Promise<WorkItemLease> {
  await mockDelay(80)
  const seed = PROCUREMENT_CONFIRMATION_SEED.find(
    (t) => t.workItemId === workItemId
  )
  if (!seed) throw new Error("任务不存在")
  if (isProcurementWorkItemTerminal(workItemId)) {
    throw new Error("任务已完成，无法领取")
  }
  try {
    const lease = claimWorkItemSession({
      workItemId,
      subjectVersion: seed.subjectVersion,
      subjectHash: seed.salesSubmission.subjectHash,
      leaseVersion: seed.lease?.leaseVersion ?? 1,
      ownerUserId: "user_procurement",
    })
    return {
      workItemId,
      claimedByLabel: "当前用户 · 李采购",
      expiresAt: lease.leaseExpiresAt,
      leaseVersion: lease.leaseVersion,
      claimToken: lease.claimToken,
    }
  } catch (error) {
    if (error instanceof WorkItemMockError) throw new Error(error.message)
    throw error
  }
}

export async function renewProcurementWorkItem(input: {
  workItemId: string
  claimToken: string
}): Promise<WorkItemLease> {
  await mockDelay(60)
  const seed = PROCUREMENT_CONFIRMATION_SEED.find(
    (t) => t.workItemId === input.workItemId
  )
  if (!seed) throw new Error("任务不存在")
  const existing = getSessionLease(input.workItemId)
  if (!existing || existing.claimToken !== input.claimToken) {
    throw new Error("租约无效，请重新领取")
  }
  // 续租 = 同用户再次 claim，签发新 token / 升 leaseVersion
  clearSessionLease(input.workItemId)
  return claimProcurementWorkItem(input.workItemId)
}

export async function saveProcurementConfirmation(input: {
  workItemId: string
  confirmationId: string
  submissionId: string
  expectedEditVersion: number
  claimToken: string
  leaseVersion: number
  lines: ConfirmationLineDraft[]
  idempotencyKey: string
}): Promise<{ editVersion: number }> {
  await mockDelay(100)
  const seed = PROCUREMENT_CONFIRMATION_SEED.find(
    (t) => t.workItemId === input.workItemId
  )
  if (!seed) throw new Error("任务不存在")
  try {
    // WorkItemActionEnvelope · SAVE — 非终结
    applyWorkItemActionSession({
      workItemId: input.workItemId,
      claimToken: input.claimToken,
      leaseVersion: input.leaseVersion,
      expectedSubjectHash: seed.salesSubmission.subjectHash,
      idempotencyKey: input.idempotencyKey,
      action: { kind: "SAVE_EVIDENCE", note: "保存采购确认分行" },
    })
    return saveProcurementDraft(
      input.workItemId,
      input.lines,
      input.expectedEditVersion
    )
  } catch (error) {
    if (error instanceof WorkItemMockError) throw new Error(error.message)
    throw error
  }
}

export async function completeProcurementDecision(input: {
  workItemId: string
  claimToken: string
  leaseVersion: number
  expectedSubjectHash: string
  expectedSubjectVersion: string
  idempotencyKey: string
  forceUnknown?: boolean
  decision:
    | {
        reviewResult: "APPROVED"
        confirmationId: string
        submissionId: string
        expectedConfirmationEditVersion: number
        salesOrderId: string
        salesOrderNo: string
        subjectHash: string
      }
    | {
        reviewResult: "REJECTED"
        confirmationId: string
        submissionId: string
        expectedConfirmationEditVersion: number
        salesOrderId: string
        salesOrderNo: string
        subjectHash: string
        rejectReasonCode: RejectReasonCode
        comment: string
      }
}): Promise<FormalActionResponse> {
  await mockDelay(150)

  const cachedBiz = getProcurementBusinessOutcome(input.workItemId)
  const idem = getIdempotencyEntry(input.idempotencyKey)
  if (idem?.state === "succeeded") {
    const payload = idem.payload as { formalOutcome?: FormalOutcome }
    if (payload?.formalOutcome) {
      return { status: "succeeded", outcome: payload.formalOutcome }
    }
    if (cachedBiz) {
      return { status: "succeeded", outcome: cachedBiz as FormalOutcome }
    }
  }
  if (idem?.state === "pending" || input.forceUnknown) {
    try {
      if (input.forceUnknown) {
        completeWorkItemSession({
          workItemId: input.workItemId,
          claimToken: input.claimToken,
          leaseVersion: input.leaseVersion,
          expectedSubjectHash: input.expectedSubjectHash,
          idempotencyKey: input.idempotencyKey,
          decision: {
            kind:
              input.decision.reviewResult === "APPROVED"
                ? "APPROVED_AND_SALES_EFFECTIVE"
                : "REJECTED_TO_SALES",
          },
          simulateTimeout: true,
        })
      }
    } catch (error) {
      if (error instanceof WorkItemMockError && error.code === "TIMEOUT") {
        return {
          status: "unknown",
          message:
            "正式结果尚未确定。请勿假定已通过或已驳回，停留当前项并使用同一幂等键查询。",
          idempotencyKey: input.idempotencyKey,
        }
      }
    }
    if (idem?.state === "pending") {
      return {
        status: "unknown",
        message:
          "正式结果尚未确定。请勿假定已通过或已驳回，停留当前项并使用同一幂等键查询。",
        idempotencyKey: input.idempotencyKey,
      }
    }
  }

  const seed = PROCUREMENT_CONFIRMATION_SEED.find(
    (t) => t.workItemId === input.workItemId
  )
  if (!seed) {
    return { status: "failed", code: "NOT_FOUND", message: "任务不存在" }
  }

  if (input.decision.reviewResult === "APPROVED") {
    if (seed.salesSubmission.subjectHash !== input.expectedSubjectHash) {
      return {
        status: "failed",
        code: "SUBJECT_HASH_MISMATCH",
        message: "销售提交指纹已变化，请刷新后处理最新提交",
      }
    }
    const projected = projectTask(seed)
    if (!projected) {
      return { status: "failed", code: "ALREADY_DONE", message: "任务已完成" }
    }
    if (projected.decisionSummary.blockingIssues.length > 0) {
      return {
        status: "failed",
        code: "VALIDATION_BLOCKED",
        message: projected.decisionSummary.blockingIssues
          .map((i) => i.message)
          .join("；"),
      }
    }

    const outcome: FormalOutcome = {
      kind: "APPROVED_AND_SALES_EFFECTIVE",
      procurementConfirmationId: input.decision.confirmationId,
      salesOrderId: input.decision.salesOrderId,
      salesOrderNo: input.decision.salesOrderNo,
      submissionId: input.decision.submissionId,
      subjectHash: input.decision.subjectHash,
      salesOrderRevisionId: `rev_${input.decision.submissionId}`,
      receivableAccountId: `recv_${input.decision.salesOrderId}`,
      procurementCreationBasisId: `pcb_${input.decision.confirmationId}`,
      reference: `PC-OK-${input.workItemId.toUpperCase()}`,
    }

    try {
      completeWorkItemSession({
        workItemId: input.workItemId,
        claimToken: input.claimToken,
        leaseVersion: input.leaseVersion,
        expectedSubjectHash: input.expectedSubjectHash,
        idempotencyKey: input.idempotencyKey,
        decision: {
          kind: "APPROVED_AND_SALES_EFFECTIVE",
          summary: `采购确认通过；销售 ${input.decision.salesOrderNo} 生效；创建依据 ${outcome.procurementCreationBasisId}`,
        },
      })
      // 把业务结果挂到幂等 payload 与 workItem 映射，供重放
      setProcurementBusinessOutcome(input.workItemId, outcome)
      markQueueTaskCompleted("W07", input.workItemId)
      // 覆盖通用 complete 的幂等 payload，附带 formalOutcome
      const entry = getIdempotencyEntry(input.idempotencyKey)
      if (entry?.state === "succeeded") {
        // re-store with formalOutcome
        setIdempotencySucceeded(input.idempotencyKey, "COMPLETE", {
          ...(entry.payload as object),
          formalOutcome: outcome,
        })
      }
      return { status: "succeeded", outcome }
    } catch (error) {
      if (error instanceof WorkItemMockError) {
        if (error.code === "TIMEOUT") {
          return {
            status: "unknown",
            message: error.message,
            idempotencyKey: input.idempotencyKey,
          }
        }
        return {
          status: "failed",
          code: error.code,
          message: error.message,
        }
      }
      throw error
    }
  }

  // REJECTED
  const outcome: FormalOutcome = {
    kind: "REJECTED_TO_SALES",
    procurementConfirmationId: input.decision.confirmationId,
    salesOrderId: input.decision.salesOrderId,
    salesOrderNo: input.decision.salesOrderNo,
    rejectedSubmissionId: input.decision.submissionId,
    rejectedSubjectHash: input.decision.subjectHash,
    workflowActionId: `wa_rej_${input.workItemId}`,
    nextSalesResolutions: [
      "RESUBMIT_CHANGED_TERMS",
      "REQUEST_LOW_MARGIN_ACCEPTANCE",
      "VOID_AFTER_REJECTION",
    ],
    reference: `PC-REJ-${input.workItemId.toUpperCase()}`,
    rejectReasonCode: input.decision.rejectReasonCode,
    comment: input.decision.comment,
  }

  try {
    completeWorkItemSession({
      workItemId: input.workItemId,
      claimToken: input.claimToken,
      leaseVersion: input.leaseVersion,
      expectedSubjectHash: input.expectedSubjectHash,
      idempotencyKey: input.idempotencyKey,
      decision: {
        kind: "REJECTED_TO_SALES",
        summary: `采购确认驳回；无后继任务；销售三路待 W05 处理`,
        note: input.decision.comment,
      },
    })
    setProcurementBusinessOutcome(input.workItemId, outcome)
    markQueueTaskCompleted("W07", input.workItemId)
    const entry = getIdempotencyEntry(input.idempotencyKey)
    if (entry?.state === "succeeded") {
      setIdempotencySucceeded(input.idempotencyKey, "COMPLETE", {
        ...(entry.payload as object),
        formalOutcome: outcome,
      })
    }
    return { status: "succeeded", outcome }
  } catch (error) {
    if (error instanceof WorkItemMockError) {
      if (error.code === "TIMEOUT") {
        return {
          status: "unknown",
          message: error.message,
          idempotencyKey: input.idempotencyKey,
        }
      }
      return { status: "failed", code: error.code, message: error.message }
    }
    throw error
  }
}

export async function deferProcurementConfirmation(input: {
  workItemId: string
  claimToken: string
  leaseVersion: number
  queueContextId: string
  nextWorkItemId?: string
  idempotencyKey: string
}): Promise<FormalActionResponse> {
  await mockDelay(100)
  const seed = PROCUREMENT_CONFIRMATION_SEED.find(
    (t) => t.workItemId === input.workItemId
  )
  if (!seed) {
    return { status: "failed", code: "NOT_FOUND", message: "任务不存在" }
  }
  try {
    applyWorkItemActionSession({
      workItemId: input.workItemId,
      claimToken: input.claimToken,
      leaseVersion: input.leaseVersion,
      expectedSubjectHash: seed.salesSubmission.subjectHash,
      idempotencyKey: input.idempotencyKey,
      action: { kind: "DEFER", note: "采购确认暂挂" },
    })
    markQueueTaskHeld("W07", input.workItemId)
    const outcome: FormalOutcome = {
      kind: "DEFERRED",
      workItemId: input.workItemId,
      workItemStatus: "PENDING",
      leaseDisposition: "RELEASED",
      nextWorkItemId: input.nextWorkItemId,
      reference: `PC-HOLD-${input.workItemId.toUpperCase()}`,
    }
    return { status: "succeeded", outcome }
  } catch (error) {
    if (error instanceof WorkItemMockError) {
      return { status: "failed", code: error.code, message: error.message }
    }
    throw error
  }
}

export async function resolveUnknownProcurementResult(input: {
  idempotencyKey: string
  settle?: boolean
  settlePayload?: Parameters<typeof completeProcurementDecision>[0]
}): Promise<FormalActionResponse> {
  await mockDelay(80)
  const entry = queryIdempotencyResult(input.idempotencyKey)
  if (entry?.state === "succeeded") {
    const payload = entry.payload as { formalOutcome?: FormalOutcome }
    if (payload?.formalOutcome) {
      return { status: "succeeded", outcome: payload.formalOutcome }
    }
  }
  if (entry?.state === "pending" && input.settle && input.settlePayload) {
    const decision = input.settlePayload.decision
    try {
      finalizePendingComplete({
        idempotencyKey: input.idempotencyKey,
        workItemId: input.settlePayload.workItemId,
        expectedSubjectHash: input.settlePayload.expectedSubjectHash,
        decision: {
          kind:
            decision.reviewResult === "APPROVED"
              ? "APPROVED_AND_SALES_EFFECTIVE"
              : "REJECTED_TO_SALES",
        },
      })
      // Build outcome from settle payload
      if (decision.reviewResult === "APPROVED") {
        const outcome: FormalOutcome = {
          kind: "APPROVED_AND_SALES_EFFECTIVE",
          procurementConfirmationId: decision.confirmationId,
          salesOrderId: decision.salesOrderId,
          salesOrderNo: decision.salesOrderNo,
          submissionId: decision.submissionId,
          subjectHash: decision.subjectHash,
          salesOrderRevisionId: `rev_${decision.submissionId}`,
          receivableAccountId: `recv_${decision.salesOrderId}`,
          procurementCreationBasisId: `pcb_${decision.confirmationId}`,
          reference: `PC-OK-${input.settlePayload.workItemId.toUpperCase()}`,
        }
        setProcurementBusinessOutcome(input.settlePayload.workItemId, outcome)
        markQueueTaskCompleted("W07", input.settlePayload.workItemId)
        setIdempotencySucceeded(input.idempotencyKey, "COMPLETE", {
          formalOutcome: outcome,
        })
        return { status: "succeeded", outcome }
      }
      const outcome: FormalOutcome = {
        kind: "REJECTED_TO_SALES",
        procurementConfirmationId: decision.confirmationId,
        salesOrderId: decision.salesOrderId,
        salesOrderNo: decision.salesOrderNo,
        rejectedSubmissionId: decision.submissionId,
        rejectedSubjectHash: decision.subjectHash,
        workflowActionId: `wa_rej_${input.settlePayload.workItemId}`,
        nextSalesResolutions: [
          "RESUBMIT_CHANGED_TERMS",
          "REQUEST_LOW_MARGIN_ACCEPTANCE",
          "VOID_AFTER_REJECTION",
        ],
        reference: `PC-REJ-${input.settlePayload.workItemId.toUpperCase()}`,
        rejectReasonCode: decision.rejectReasonCode,
        comment: decision.comment,
      }
      setProcurementBusinessOutcome(input.settlePayload.workItemId, outcome)
      markQueueTaskCompleted("W07", input.settlePayload.workItemId)
      setIdempotencySucceeded(input.idempotencyKey, "COMPLETE", {
        formalOutcome: outcome,
      })
      return { status: "succeeded", outcome }
    } catch (error) {
      if (error instanceof WorkItemMockError) {
        return { status: "failed", code: error.code, message: error.message }
      }
      throw error
    }
  }
  if (entry?.state === "pending") {
    return {
      status: "unknown",
      message: "仍在处理中，正式结果未知。停留当前项。",
      idempotencyKey: input.idempotencyKey,
    }
  }
  return {
    status: "failed",
    code: "NO_PENDING",
    message: "未找到该幂等键对应的处理中请求",
  }
}

export function getTerminalOutcome(workItemId: string): FormalOutcome | null {
  const biz = getProcurementBusinessOutcome(workItemId)
  return (biz as FormalOutcome) ?? null
}
