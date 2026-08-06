/**
 * W07 采购二次确认 · 真实 HTTP API。
 * 契约形状保持 features/procurement-confirmation/types.ts 与 queries.ts 不变；
 * 后端差异在本文件内适配，缺口登记见 docs/dev-plan/p4-evidence/F4.md。
 */

import { apiGet, apiPost, apiPut } from "@/lib/api"
import type { ApiError } from "@/lib/api"
import type {
  ConfirmationLineDraft,
  CoverageByLine,
  FormalActionResponse,
  FormalOutcome,
  FulfillmentMode,
  ProcurementConfirmationTask,
  ProcurementQueueView,
  RejectReasonCode,
  SubmissionOrigin,
  WorkItemLease,
} from "@/features/procurement-confirmation/types"

export type QueueFilters = {
  scope: "mine" | "role_pool"
  due?: "active" | "today" | "overdue"
  sort?: "due_at" | "submitted_at" | "priority"
  orderNo?: string
  currentWorkItemId?: string
  queueContextId?: string
}

// ─── Backend DTO shapes (snake_case wire format) ─────────────────────────────

type BackendPage<T> = {
  items: T[]
  total: number
  page?: number
  page_size?: number
}

type BackendWorkItem = {
  id: string
  work_item_type: string
  business_object_type: string
  business_object_id: string
  subject_version?: string | null
  status: "UNCLAIMED" | "IN_PROGRESS" | "COMPLETED" | "CLOSED" | string
  owner_role?: string | null
  owner_user_id?: string | null
  priority?: "urgent" | "high" | "normal" | "low" | string
  due_at?: number | null
  reason_code?: string | null
  impact_summary?: string | null
  completion_action?: string
  version: number
  created_at: number
}

type BackendConfirmationLine = {
  id: string
  line_no: number
  sales_order_submission_line_id: string
  supplier_id: string
  confirmed_quantity: string
  latest_cost_gross: string
  input_tax_rate: string
  expected_delivery_date: string
  fulfillment_mode: FulfillmentMode | string
  supplier_capability_revision_id: string
}

type BackendConfirmationDetail = {
  id: string
  sales_order_id: string
  submission_id: string
  status: "PENDING" | "APPROVED" | "REJECTED" | string
  handled_by?: string | null
  handled_at?: number | null
  version: number
  created_at: number
  lines: BackendConfirmationLine[]
}

type BackendDecisionView = {
  confirmation_id: string
  sales_order_id: string
  status: string
  revision_id?: string | null
  receivable_account_id?: string | null
  handled_at: number
  reference: string
}

type BackendSalesOrderDetail = {
  id: string
  order_no: string
  customer_id?: string
  contract_id?: string | null
  submissions?: Array<{
    id: string
    submission_no: number
    status?: string
    gross_amount?: string
    net_amount?: string
    tax_amount?: string
    submitted_by?: string
    submitted_at?: number
  }>
  working_copy?: {
    gross_amount?: string
    lines?: Array<{
      id: string
      sales_order_line_id?: string
      line_no: number
      item_name_snapshot?: string
      unit_snapshot?: string | null
      quantity?: string | null
      gross_amount?: string
    }>
  } | null
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

function isApiError(error: unknown): error is ApiError {
  return (
    typeof error === "object" &&
    error !== null &&
    "kind" in error &&
    "message" in error
  )
}

function apiErrorMessage(error: unknown): string {
  if (!isApiError(error)) {
    return error instanceof Error ? error.message : "请求失败"
  }
  const data = error.responseData as
    | { errorMessage?: string }
    | undefined
  if (
    data &&
    typeof data.errorMessage === "string" &&
    data.errorMessage &&
    data.errorMessage !== "OK"
  ) {
    return data.errorMessage
  }
  return error.message
}

function apiErrorCode(error: unknown): string {
  if (!isApiError(error)) return "REQUEST_FAILED"
  if (error.status === 409) return "CONFLICT"
  if (error.status === 404) return "NOT_FOUND"
  if (error.status === 403) return "FORBIDDEN"
  if (error.status === 422) return "UNPROCESSABLE"
  if (error.kind === "Validation") return "VALIDATION"
  return error.kind.toUpperCase()
}

function secsToIso(secs?: number | null): string {
  if (secs == null || secs <= 0) return ""
  return new Date(secs * 1000).toISOString()
}

function priorityToNumber(p?: string | null): number {
  switch (p) {
    case "urgent":
      return 100
    case "high":
      return 80
    case "normal":
      return 50
    case "low":
      return 20
    default:
      return 50
  }
}

function mapRejectReasonToBackend(
  code: RejectReasonCode
): string {
  switch (code) {
    case "UNFULFILLABLE":
      return "CANNOT_FULFILL"
    case "COST_INCREASE":
      return "COST_INCREASE"
    case "DELIVERY_UNMET":
      return "DELIVERY_NOT_MET"
    case "QUALIFICATION_INVALID":
      return "QUALIFICATION_EXPIRED"
    default:
      return "OTHER"
  }
}

function mapFulfillmentMode(mode: string): FulfillmentMode {
  if (
    mode === "WAREHOUSE" ||
    mode === "SUPPLIER_DIRECT" ||
    mode === "ELECTRONIC" ||
    mode === "SERVICE"
  ) {
    return mode
  }
  return "WAREHOUSE"
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

function emptyCoverageFromLines(
  lines: readonly ConfirmationLineDraft[],
  submissionLineIds: readonly { id: string; name: string; required: string }[]
): {
  coverageByLine: CoverageByLine[]
  estimatedPurchaseGross: string
  blockingIssues: ProcurementConfirmationTask["decisionSummary"]["blockingIssues"]
  warnings: ProcurementConfirmationTask["decisionSummary"]["warnings"]
} {
  // 覆盖/毛利由服务端裁决；此处仅做展示占位，不重算金额（P4 §2.7）。
  // 缺口：后端详情未返回 decisionSummary / coverageByLine。
  const coverageByLine: CoverageByLine[] = submissionLineIds.map((s) => {
    const confirmedLines = lines.filter(
      (l) => l.submissionLineId === s.id
    )
    const confirmed =
      confirmedLines.length > 0
        ? confirmedLines
            .map((l) => l.confirmedQuantity)
            .join("+") || "0"
        : "0"
    return {
      submissionLineId: s.id,
      itemName: s.name,
      confirmed,
      required: s.required,
      complete: confirmedLines.length > 0,
      gap: confirmedLines.length > 0 ? "0" : s.required,
    }
  })
  return {
    coverageByLine,
    estimatedPurchaseGross: "—",
    blockingIssues: [],
    warnings: [],
  }
}

async function fetchWorkItemsForQueue(
  filters: QueueFilters
): Promise<BackendWorkItem[]> {
  // mine ≈ 已领取进行中；role_pool ≈ 待领取。后端无 responsibility_scope 字段。
  const status =
    filters.scope === "mine" ? "IN_PROGRESS" : "UNCLAIMED"
  const page = await apiGet<BackendPage<BackendWorkItem>>(
    "/admin/work-items",
    {
      work_item_type: "PROCUREMENT_CONFIRMATION",
      status,
      page: 1,
      page_size: 100,
      sort_by: filters.sort === "due_at" ? "due_at" : "created_at",
      sort_dir: filters.sort === "due_at" ? "asc" : "desc",
    }
  )
  return page.items ?? []
}

async function fetchConfirmationDetail(
  confirmationId: string
): Promise<BackendConfirmationDetail | null> {
  try {
    return await apiGet<BackendConfirmationDetail>(
      `/admin/procurement-confirmations/${encodeURIComponent(confirmationId)}`
    )
  } catch (error) {
    if (isApiError(error) && error.status === 404) return null
    throw error
  }
}

async function fetchSalesOrderDetail(
  salesOrderId: string
): Promise<BackendSalesOrderDetail | null> {
  try {
    return await apiGet<BackendSalesOrderDetail>(
      `/admin/sales-orders/${encodeURIComponent(salesOrderId)}`
    )
  } catch {
    // 跨域只读增强；失败时降级为空摘要（登记缺口）
    return null
  }
}

function mapConfirmationLines(
  lines: BackendConfirmationLine[]
): ConfirmationLineDraft[] {
  return lines.map((line) => ({
    lineKey: line.id,
    submissionLineId: line.sales_order_submission_line_id,
    supplierId: line.supplier_id,
    supplierName: line.supplier_id,
    confirmedQuantity: String(line.confirmed_quantity ?? "0"),
    latestCostGross: String(line.latest_cost_gross ?? "0"),
    inputTaxRate: String(line.input_tax_rate ?? "0"),
    expectedDeliveryDate: line.expected_delivery_date ?? "",
    fulfillmentMode: mapFulfillmentMode(String(line.fulfillment_mode)),
    capabilityRevisionId: line.supplier_capability_revision_id ?? "",
    capabilitySummary: "",
    // 后端未返回资质状态
    qualificationStatus: "VALID" as const,
  }))
}

async function projectTask(
  workItem: BackendWorkItem
): Promise<ProcurementConfirmationTask | null> {
  if (
    workItem.status === "COMPLETED" ||
    workItem.status === "CLOSED"
  ) {
    return null
  }

  const confirmationId = workItem.business_object_id
  const detail = await fetchConfirmationDetail(confirmationId)
  if (!detail) return null
  if (detail.status === "APPROVED" || detail.status === "REJECTED") {
    return null
  }

  const sales = await fetchSalesOrderDetail(detail.sales_order_id)
  const submission =
    sales?.submissions?.find((s) => s.id === detail.submission_id) ??
    sales?.submissions?.[0]

  const confLines = mapConfirmationLines(detail.lines ?? [])

  // 销售提交行：后端 submission 无明细；尽量用 working_copy 行作只读参考（缺口）
  const submissionLines =
    sales?.working_copy?.lines?.map((line) => ({
      submissionLineId: line.id,
      itemName: line.item_name_snapshot ?? `行 ${line.line_no}`,
      itemSku: "",
      committedQuantity: String(line.quantity ?? "0"),
      unit: line.unit_snapshot ?? "",
      requestedDeliveryDate: "",
      salesAmountGross: String(line.gross_amount ?? "0"),
    })) ??
    confLines.map((line) => ({
      submissionLineId: line.submissionLineId,
      itemName: "销售明细",
      itemSku: "",
      committedQuantity: line.confirmedQuantity,
      unit: "",
      requestedDeliveryDate: "",
      salesAmountGross: "0",
    }))

  const coverage = emptyCoverageFromLines(
    confLines,
    submissionLines.map((s) => ({
      id: s.submissionLineId,
      name: s.itemName,
      required: s.committedQuantity,
    }))
  )

  const status: ProcurementConfirmationTask["status"] =
    workItem.status === "IN_PROGRESS" ? "IN_PROGRESS" : "PENDING"

  const origin: SubmissionOrigin = "INITIAL"

  return {
    workItemId: workItem.id,
    responsibilityScope:
      workItem.status === "IN_PROGRESS" ? "mine" : "role_pool",
    status,
    priority: priorityToNumber(workItem.priority),
    dueAt: secsToIso(workItem.due_at) || secsToIso(workItem.created_at),
    impactSummary: workItem.impact_summary ?? "采购二次确认",
    subjectVersion:
      workItem.subject_version ??
      String(detail.version) ??
      detail.submission_id,
    subjectHash: detail.submission_id,
    held: false,
    lease:
      workItem.status === "IN_PROGRESS" && workItem.owner_user_id
        ? { claimedByLabel: workItem.owner_user_id }
        : undefined,
    salesSubmission: {
      salesOrderId: detail.sales_order_id,
      salesOrderNo: sales?.order_no ?? detail.sales_order_id,
      submissionId: detail.submission_id,
      submissionNo: submission?.submission_no ?? 0,
      subjectHash: detail.submission_id,
      subjectHashSummary: (detail.submission_id ?? "").slice(0, 12),
      submittedAt: secsToIso(submission?.submitted_at) || secsToIso(detail.created_at),
      submittedByLabel: submission?.submitted_by ?? "—",
      customerSnapshot: sales?.customer_id ?? "—",
      contractSnapshot: sales?.contract_id ?? undefined,
      paymentTermLabel: "—",
      grossAmount: String(
        submission?.gross_amount ??
          sales?.working_copy?.gross_amount ??
          "0"
      ),
      origin,
      lines: submissionLines,
    },
    confirmation: {
      confirmationId: detail.id,
      status: "PENDING",
      editVersion: detail.version,
      lines: confLines,
    },
    decisionSummary: {
      coverageByLine: coverage.coverageByLine,
      estimatedPurchaseGross: coverage.estimatedPurchaseGross,
      estimatedMargin: undefined,
      marginDelta: undefined,
      blockingIssues: coverage.blockingIssues,
      warnings: coverage.warnings,
    },
    allowedActions:
      workItem.status === "IN_PROGRESS"
        ? ["SAVE", "APPROVE", "REJECT", "DEFER"]
        : ["CLAIM"],
    actionBlockers: [],
    riskLabel: workItem.status === "IN_PROGRESS" ? "处理中" : "待领取",
    riskTone: workItem.status === "IN_PROGRESS" ? "info" : "warning",
    riskDescription: workItem.impact_summary ?? "",
  }
}

// ─── Public API (stable signatures) ──────────────────────────────────────────

export async function fetchProcurementQueue(
  filters: QueueFilters
): Promise<ProcurementQueueView> {
  const workItems = await fetchWorkItemsForQueue(filters)

  const projected = (
    await Promise.all(workItems.map((wi) => projectTask(wi)))
  ).filter((t): t is ProcurementConfirmationTask => t != null)

  let tasks = projected

  if (filters.orderNo) {
    const q = filters.orderNo.trim().toUpperCase()
    tasks = tasks.filter((t) =>
      t.salesSubmission.salesOrderNo.toUpperCase().includes(q)
    )
  }

  // due 筛选：后端列表无 due 过滤；在已取页内适配（分页准确性缺口）
  const now = Date.now()
  const today = new Date().toISOString().slice(0, 10)
  if (filters.due === "overdue") {
    tasks = tasks.filter((t) => {
      if (!t.dueAt) return false
      return new Date(t.dueAt).getTime() < now
    })
  } else if (filters.due === "today") {
    tasks = tasks.filter((t) => t.dueAt.slice(0, 10) === today)
  }

  tasks = sortTasks(tasks, filters.sort ?? "due_at")

  const queueContextId =
    filters.queueContextId ??
    `queue:procurement-confirmation:${filters.scope}`

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
    tasks.length === 0
      ? projected.length === 0 && workItems.length === 0
        ? "NO_TASKS"
        : "FILTER_NO_RESULT"
      : undefined

  return {
    preferences: {
      autoNextDefault: true,
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
  const detail = await apiGet<BackendWorkItem>(
    `/admin/work-items/${encodeURIComponent(workItemId)}`
  )
  await apiPost<BackendWorkItem>(
    `/admin/work-items/${encodeURIComponent(workItemId)}/claim`,
    { version: detail.version }
  )
  return {
    workItemId,
    claimedByLabel: detail.owner_user_id ?? "当前用户",
  }
}

export async function saveProcurementConfirmation(input: {
  workItemId: string
  confirmationId: string
  submissionId: string
  expectedEditVersion: number
  lines: ConfirmationLineDraft[]
}): Promise<{ editVersion: number }> {
  const body = {
    version: input.expectedEditVersion,
    lines: input.lines.map((line, index) => ({
      line_no: index + 1,
      sales_order_submission_line_id: line.submissionLineId,
      supplier_id: line.supplierId,
      confirmed_quantity: line.confirmedQuantity,
      latest_cost_gross: line.latestCostGross,
      input_tax_rate: line.inputTaxRate,
      expected_delivery_date: line.expectedDeliveryDate,
      fulfillment_mode: line.fulfillmentMode,
      supplier_capability_revision_id: line.capabilityRevisionId,
    })),
  }

  const detail = await apiPut<BackendConfirmationDetail>(
    `/admin/procurement-confirmations/${encodeURIComponent(input.confirmationId)}/lines`,
    body
  )
  return { editVersion: detail.version }
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
  try {
    if (input.decision.reviewResult === "APPROVED") {
      const data = await apiPost<BackendDecisionView>(
        `/admin/procurement-confirmations/${encodeURIComponent(input.decision.confirmationId)}/approve`,
        {
          idempotency_key: `pc-approve-${input.workItemId}-${input.decision.expectedConfirmationEditVersion}`,
        }
      )
      const outcome: FormalOutcome = {
        kind: "APPROVED_AND_SALES_EFFECTIVE",
        procurementConfirmationId: data.confirmation_id,
        salesOrderId: data.sales_order_id,
        salesOrderNo: input.decision.salesOrderNo,
        submissionId: input.decision.submissionId,
        subjectHash: input.decision.subjectHash,
        salesOrderRevisionId: data.revision_id ?? "",
        receivableAccountId: data.receivable_account_id ?? "",
        procurementCreationBasisId: data.confirmation_id,
        reference: data.reference,
      }
      return { status: "succeeded", outcome }
    }

    const data = await apiPost<BackendDecisionView>(
      `/admin/procurement-confirmations/${encodeURIComponent(input.decision.confirmationId)}/reject`,
      {
        reject_reason_code: mapRejectReasonToBackend(
          input.decision.rejectReasonCode
        ),
        comment: input.decision.comment,
        idempotency_key: `pc-reject-${input.workItemId}-${input.decision.expectedConfirmationEditVersion}`,
      }
    )
    const outcome: FormalOutcome = {
      kind: "REJECTED_TO_SALES",
      procurementConfirmationId: data.confirmation_id,
      salesOrderId: data.sales_order_id,
      salesOrderNo: input.decision.salesOrderNo,
      rejectedSubmissionId: input.decision.submissionId,
      rejectedSubjectHash: input.decision.subjectHash,
      workflowActionId: data.reference,
      nextSalesResolutions: [
        "RESUBMIT_CHANGED_TERMS",
        "REQUEST_LOW_MARGIN_ACCEPTANCE",
        "VOID_AFTER_REJECTION",
      ],
      reference: data.reference,
      rejectReasonCode: input.decision.rejectReasonCode,
      comment: input.decision.comment,
    }
    return { status: "succeeded", outcome }
  } catch (error) {
    return {
      status: "failed",
      message: apiErrorMessage(error),
      code: apiErrorCode(error),
    }
  }
}

export async function deferProcurementConfirmation(input: {
  workItemId: string
  queueContextId: string
  nextWorkItemId?: string
}): Promise<FormalActionResponse> {
  try {
    const detail = await apiGet<BackendWorkItem>(
      `/admin/work-items/${encodeURIComponent(input.workItemId)}`
    )
    await apiPost<BackendWorkItem>(
      `/admin/work-items/${encodeURIComponent(input.workItemId)}/defer`,
      {
        version: detail.version,
        comment: "采购确认暂挂",
      }
    )
    const outcome: FormalOutcome = {
      kind: "DEFERRED",
      workItemId: input.workItemId,
      workItemStatus: "PENDING",
      leaseDisposition: "RELEASED",
      nextWorkItemId: input.nextWorkItemId,
      reference: `PC-HOLD-${input.workItemId}`,
    }
    return { status: "succeeded", outcome }
  } catch (error) {
    return {
      status: "failed",
      message: apiErrorMessage(error),
      code: apiErrorCode(error),
    }
  }
}

/** 终局结果查询：后端无按 workItem 反查业务 outcome 的独立接口。 */
export function getTerminalOutcome(workItemId: string): FormalOutcome | null {
  void workItemId
  return null
}
