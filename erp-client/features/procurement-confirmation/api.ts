/**
 * W07 session-mock API：queryFn / mutationFn 纯函数。
 * 租约 / 完成 / 暂挂复用 W02 session-state 统一信封语义。
 */

import { mockDelay } from "@/lib/mock-delay"
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
  completeWorkItemSession,
  getProcurementBusinessOutcome,
  getProcurementDraft,
  getSessionLease,
  isProcurementWorkItemHeld,
  isProcurementWorkItemTerminal,
  markQueueTaskCompleted,
  markQueueTaskHeld,
  saveProcurementDraft,
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
  const publicLease = getSessionLease(seed.workItemId)
  const lines = (draft?.lines as ConfirmationLineDraft[] | undefined) ??
    seed.confirmation.lines
  const editVersion = draft?.editVersion ?? seed.confirmation.editVersion
  const decision = recomputeCoverage(seed, lines)

  return {
    ...seed,
    status: held ? "IN_PROGRESS" : seed.status,
    held,
    // 查询 View：仅公开处理人信息
    lease: publicLease
      ? {
          claimedByLabel: "当前用户 · 李采购",
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
              message: "存在未完整覆盖的销售明细，系统将拒绝通过",
            },
          ]
        : [],
    riskLabel: held ? "已跳过" : seed.riskLabel,
    riskTone: held ? "warning" : seed.riskTone,
  }
}

function filterSummary(filters: QueueFilters): string {
  const parts = [
    filters.scope === "mine" ? "仅我的" : "团队",
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
    claimWorkItemSession({
      workItemId,
      subjectVersion: seed.subjectVersion,
      ownerUserId: "user_procurement",
    })
    return {
      workItemId,
      claimedByLabel: "当前用户 · 李采购",
    }
  } catch (error) {
    if (error instanceof WorkItemMockError) throw new Error(error.message)
    throw error
  }
}

export async function saveProcurementConfirmation(input: {
  workItemId: string
  confirmationId: string
  submissionId: string
  expectedEditVersion: number
  lines: ConfirmationLineDraft[]
}): Promise<{ editVersion: number }> {
  await mockDelay(100)
  const seed = PROCUREMENT_CONFIRMATION_SEED.find(
    (t) => t.workItemId === input.workItemId
  )
  if (!seed) throw new Error("任务不存在")
  try {
    // 统一动作命令 · SAVE — 非终结
    applyWorkItemActionSession({
      workItemId: input.workItemId,
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
  expectedSubjectVersion: string
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

  const seed = PROCUREMENT_CONFIRMATION_SEED.find(
    (t) => t.workItemId === input.workItemId
  )
  if (!seed) {
    return { status: "failed", code: "NOT_FOUND", message: "任务不存在" }
  }

  if (input.decision.reviewResult === "APPROVED") {
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
      reference: `PC-OK-${input.decision.salesOrderNo}`,
    }

    try {
      completeWorkItemSession({
        workItemId: input.workItemId,
        expectedSubjectVersion: input.expectedSubjectVersion,
        decision: {
          kind: "APPROVED_AND_SALES_EFFECTIVE",
          summary: `采购确认通过；销售 ${input.decision.salesOrderNo} 生效；创建依据 ${outcome.procurementCreationBasisId}`,
        },
      })
      // 业务结果挂到 workItem 映射，供详情查询
      setProcurementBusinessOutcome(input.workItemId, outcome)
      markQueueTaskCompleted("W07", input.workItemId)
      return { status: "succeeded", outcome }
    } catch (error) {
      if (error instanceof WorkItemMockError) {
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
    reference: `PC-REJ-${input.decision.salesOrderNo}`,
    rejectReasonCode: input.decision.rejectReasonCode,
    comment: input.decision.comment,
  }

  try {
    completeWorkItemSession({
      workItemId: input.workItemId,
      expectedSubjectVersion: input.expectedSubjectVersion,
      decision: {
        kind: "REJECTED_TO_SALES",
        summary: `采购确认驳回；无后继任务；销售三路待销售单处理`,
        note: input.decision.comment,
      },
    })
    setProcurementBusinessOutcome(input.workItemId, outcome)
    markQueueTaskCompleted("W07", input.workItemId)
    return { status: "succeeded", outcome }
  } catch (error) {
    if (error instanceof WorkItemMockError) {
      return { status: "failed", code: error.code, message: error.message }
    }
    throw error
  }
}

export async function deferProcurementConfirmation(input: {
  workItemId: string
  queueContextId: string
  nextWorkItemId?: string
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
      action: { kind: "DEFER", note: "采购确认跳过" },
    })
    markQueueTaskHeld("W07", input.workItemId)
    const outcome: FormalOutcome = {
      kind: "DEFERRED",
      workItemId: input.workItemId,
      workItemStatus: "PENDING",
      leaseDisposition: "RELEASED",
      nextWorkItemId: input.nextWorkItemId,
      reference: `PC-HOLD-${seed.salesSubmission.salesOrderNo}`,
    }
    return { status: "succeeded", outcome }
  } catch (error) {
    if (error instanceof WorkItemMockError) {
      return { status: "failed", code: error.code, message: error.message }
    }
    throw error
  }
}

export function getTerminalOutcome(workItemId: string): FormalOutcome | null {
  const biz = getProcurementBusinessOutcome(workItemId)
  return (biz as FormalOutcome) ?? null
}
