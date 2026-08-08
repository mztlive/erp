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
  ProcurementRecommendation,
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
  supplier_offering_revision_id?: string | null
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
  purchase_orders?: Array<{
    purchase_order_id: string
    purchase_no: string
  }>
  handled_at: number
  reference: string
}

type BackendSalesOrderDetail = {
  id: string
  order_no: string
  business_type?: string
  origin_system?: string
  customer_id?: string
  contract_id?: string | null
  owner_user_id?: string
  owner_user_name?: string | null
  submissions?: Array<{
    id: string
    submission_no: number
    status?: string
    customer_name: string
    contract_no?: string | null
    settlement_party_name?: string | null
    payment_term_name: string
    project_name?: string | null
    business_remark?: string | null
    gross_amount?: string
    net_amount?: string
    tax_amount?: string
    submitted_by?: string
    submitted_at?: number
    lines: Array<{
      id: string
      sales_order_line_id?: string
      line_no: number
      item_name_snapshot?: string
      spec_snapshot?: string | null
      sku_id?: string | null
      unit_snapshot?: string | null
      base_unit_code?: string | null
      quantity?: string | null
      unit_price_gross?: string | null
      sales_tax_rate?: string | null
      fulfillment_mode?: string | null
      fulfillment_due_at?: number | null
      gross_amount?: string
    }>
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

type BackendAccountProfile = {
  userid: string
  name: string
}

type BackendSupplierOffering = {
  sku_id: string
  supplier_id: string
  status: string
  current_revision_id?: string | null
  current_revision_no?: number | null
  dropship_supply_price_gross?: string | null
  bulk_supply_price_gross?: string | null
  input_tax_rate?: string | null
  bulk_minimum_order_quantity?: string | null
  availability_status?: string | null
  available_quantity?: string | null
  freight_amount?: string | null
  service_fee_amount?: string | null
  valid_from?: string | null
  valid_to?: string | null
}

type BackendRecommendation = {
  confirmation_id: string
  policy_version: string
  calculated_at: number
  ready: boolean
  lines: Array<{
    line_no: number
    sales_order_submission_line_id: string
    item_name: string
    sku_id: string
    supplier_id: string
    supplier_name: string
    supplier_offering_revision_id: string
    confirmed_quantity: string
    latest_cost_gross: string
    input_tax_rate: string
    expected_delivery_date: string
    fulfillment_mode: string
    supplier_capability_revision_id: string
    landed_gross: string
    freight_amount?: string | null
    service_fee_amount?: string | null
    recommendation_reason: string
  }>
  purchase_orders: Array<{
    supplier_id: string
    supplier_name: string
    fulfillment_mode: string
    line_count: number
    estimated_gross: string
  }>
  estimated_purchase_gross: string
  sales_gross: string
  estimated_gross_margin: string
  blocking_issues: Array<{
    code: string
    message: string
    sales_order_submission_line_id?: string | null
  }>
  warnings: Array<{
    code: string
    message: string
    sales_order_submission_line_id?: string | null
  }>
}

type BackendSupplierCapability = {
  supplier_id: string
  capability_code: string
  status: string
  current_revision_id?: string | null
  valid_from: string
  valid_to?: string | null
}

export type ProcurementSupplyOption = {
  skuId: string
  supplierId: string
  offeringRevisionId: string
  offeringRevisionNo: number
  costGross: string
  bulkCostGross: string
  dropshipCostGross: string
  bulkMinimumOrderQuantity: string
  inputTaxRate: string
  freightAmount: string
  serviceFeeAmount: string
  capabilities: Array<{
    revisionId: string
    label: string
    capabilityCode: string
  }>
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
  if (mode === "COMPANY_WAREHOUSE") return "WAREHOUSE"
  if (mode === "ELECTRONIC_DELIVERY") return "ELECTRONIC"
  if (mode === "OFFLINE_SERVICE") return "SERVICE"
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

/** 将后端最低成本方案转换为 W07 页面契约。 */
function mapRecommendation(
  recommendation: BackendRecommendation
): ProcurementRecommendation {
  const mapIssue = (issue: BackendRecommendation["blocking_issues"][number]) => ({
    code: issue.code,
    message: issue.message,
    lineId: issue.sales_order_submission_line_id ?? undefined,
  })
  return {
    confirmationId: recommendation.confirmation_id,
    policyVersion: recommendation.policy_version,
    calculatedAt: secsToIso(recommendation.calculated_at),
    ready: recommendation.ready,
    lines: recommendation.lines.map((line) => ({
      lineKey: `recommended-${line.line_no}-${line.supplier_offering_revision_id}`,
      submissionLineId: line.sales_order_submission_line_id,
      supplierId: line.supplier_id,
      supplierName: line.supplier_name,
      offeringRevisionId: line.supplier_offering_revision_id,
      confirmedQuantity: line.confirmed_quantity,
      latestCostGross: line.latest_cost_gross,
      inputTaxRate: line.input_tax_rate,
      expectedDeliveryDate: line.expected_delivery_date,
      fulfillmentMode: mapFulfillmentMode(line.fulfillment_mode),
      capabilityRevisionId: line.supplier_capability_revision_id,
      capabilitySummary: "当前有效供应商能力",
      qualificationStatus: "VALID" as const,
      itemName: line.item_name,
      itemSku: line.sku_id,
      landedGross: line.landed_gross,
      freightAmount: line.freight_amount ?? undefined,
      serviceFeeAmount: line.service_fee_amount ?? undefined,
      recommendationReason: line.recommendation_reason,
    })),
    purchaseOrders: recommendation.purchase_orders.map((order) => ({
      supplierId: order.supplier_id,
      supplierName: order.supplier_name,
      fulfillmentMode: mapFulfillmentMode(order.fulfillment_mode),
      lineCount: order.line_count,
      estimatedGross: order.estimated_gross,
    })),
    estimatedPurchaseGross: recommendation.estimated_purchase_gross,
    salesGross: recommendation.sales_gross,
    estimatedGrossMargin: recommendation.estimated_gross_margin,
    blockingIssues: recommendation.blocking_issues.map(mapIssue),
    warnings: recommendation.warnings.map(mapIssue),
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
  filters: QueueFilters,
  profile: BackendAccountProfile
): Promise<BackendWorkItem[]> {
  const status =
    filters.scope === "mine" ? "IN_PROGRESS" : "UNCLAIMED"
  const ownership =
    filters.scope === "mine"
      ? { owner_user_id: profile.userid }
      : { owner_role: "procurement" }
  const page = await apiGet<BackendPage<BackendWorkItem>>(
    "/admin/work-items",
    {
      work_item_type: "PROCUREMENT_CONFIRMATION",
      status,
      ...ownership,
      page: 1,
      page_size: 100,
      sort_by: filters.sort === "due_at" ? "due_at" : "created_at",
      sort_dir: filters.sort === "due_at" ? "asc" : "desc",
    }
  )
  return page.items ?? []
}

async function fetchAccountProfile(): Promise<BackendAccountProfile> {
  return apiGet<BackendAccountProfile>("/account/profile")
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

/** 读取后端计算的最低可执行成本采购方案。 */
export async function fetchProcurementRecommendation(
  confirmationId: string
): Promise<ProcurementRecommendation> {
  const recommendation = await apiGet<BackendRecommendation>(
    `/admin/procurement-confirmations/${encodeURIComponent(confirmationId)}/recommendation`
  )
  return mapRecommendation(recommendation)
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

/** W02 使用的采购确认业务展示摘要，避免把内部对象 ID 直接展示给用户。 */
export type ProcurementWorkItemPresentation = {
  salesOrderNo: string
  customerName: string
  contractNo?: string
  paymentTermName: string
  grossAmount: string
}

/**
 * 解析采购确认待办对应的销售提交快照。
 *
 * @param confirmationId 采购确认批次 ID。
 * @returns 可直接用于待办列表的业务摘要；对象不存在时返回 null。
 */
export async function fetchProcurementWorkItemPresentation(
  confirmationId: string
): Promise<ProcurementWorkItemPresentation | null> {
  const detail = await fetchConfirmationDetail(confirmationId)
  if (!detail) return null
  const sales = await fetchSalesOrderDetail(detail.sales_order_id)
  if (!sales) return null
  const submission = sales.submissions?.find(
    (row) => row.id === detail.submission_id
  )
  if (!submission) return null
  return {
    salesOrderNo: sales.order_no,
    customerName: submission.customer_name,
    contractNo: submission.contract_no ?? undefined,
    paymentTermName: submission.payment_term_name,
    grossAmount: String(submission.gross_amount ?? "0"),
  }
}

const CAPABILITY_LABEL: Record<string, string> = {
  physical: "实物商品",
  virtual: "虚拟商品",
  offline_service: "线下服务",
  api: "API",
  printing: "印刷",
}

/**
 * 按销售 SKU 批量读取当前有效供给及供应商能力修订。
 *
 * @param skuIds 销售提交行中的公司 SKU 集合。
 * @returns 可用于采购确认分行选择的不可变版本选项。
 */
export const fetchProcurementSupplyOptions = async (
  skuIds: readonly string[]
): Promise<ProcurementSupplyOption[]> => {
  const today = new Date().toISOString().slice(0, 10)
  const uniqueSkuIds = [...new Set(skuIds.filter(Boolean))]
  const offeringPages = await Promise.all(
    uniqueSkuIds.map((skuId) =>
      apiGet<BackendPage<BackendSupplierOffering>>(
        "/admin/supplier-catalog/offerings",
        { sku_id: skuId, page: 1, page_size: 100 }
      )
    )
  )
  const offerings = offeringPages
    .flatMap((page) => page.items)
    .filter(
      (offering) =>
        offering.status === "ACTIVE" &&
        Boolean(offering.current_revision_id) &&
        offering.availability_status === "AVAILABLE" &&
        (!offering.valid_from || offering.valid_from <= today) &&
        (!offering.valid_to || today <= offering.valid_to) &&
        (offering.available_quantity == null ||
          Number(offering.available_quantity) > 0)
    )
  const supplierIds = [...new Set(offerings.map((row) => row.supplier_id))]
  const capabilityPages = await Promise.all(
    supplierIds.map(async (supplierId) => ({
      supplierId,
      page: await apiGet<BackendPage<BackendSupplierCapability>>(
        `/admin/suppliers/${encodeURIComponent(supplierId)}/capabilities`,
        { status: "active", page: 1, page_size: 100 }
      ),
    }))
  )
  const capabilitiesBySupplier = new Map(
    capabilityPages.map(({ supplierId, page }) => [
      supplierId,
      page.items
        .filter(
          (capability) =>
            Boolean(capability.current_revision_id) &&
            capability.valid_from <= today &&
            (!capability.valid_to || today <= capability.valid_to)
        )
        .map((capability) => ({
          revisionId: capability.current_revision_id!,
          label:
            CAPABILITY_LABEL[capability.capability_code] ?? "供应商能力",
          capabilityCode: capability.capability_code,
        })),
    ])
  )
  return offerings.map((offering) => ({
    skuId: offering.sku_id,
    supplierId: offering.supplier_id,
    offeringRevisionId: offering.current_revision_id!,
    offeringRevisionNo: offering.current_revision_no ?? 1,
    costGross:
      offering.bulk_supply_price_gross ??
      offering.dropship_supply_price_gross ??
      "",
    bulkCostGross: offering.bulk_supply_price_gross ?? "",
    dropshipCostGross: offering.dropship_supply_price_gross ?? "",
    bulkMinimumOrderQuantity:
      offering.bulk_minimum_order_quantity ?? "1",
    inputTaxRate: offering.input_tax_rate ?? "",
    freightAmount: offering.freight_amount ?? "0",
    serviceFeeAmount: offering.service_fee_amount ?? "0",
    capabilities: capabilitiesBySupplier.get(offering.supplier_id) ?? [],
  }))
}

function mapConfirmationLines(
  lines: BackendConfirmationLine[]
): ConfirmationLineDraft[] {
  return lines.map((line) => ({
    lineKey: line.id,
    submissionLineId: line.sales_order_submission_line_id,
    supplierId: line.supplier_id,
    supplierName: "供应商名称加载中",
    offeringRevisionId: line.supplier_offering_revision_id ?? "",
    confirmedQuantity: String(line.confirmed_quantity ?? "0"),
    latestCostGross: String(line.latest_cost_gross ?? "0"),
    inputTaxRate: String(line.input_tax_rate ?? "0"),
    expectedDeliveryDate: line.expected_delivery_date ?? "",
    fulfillmentMode: mapFulfillmentMode(String(line.fulfillment_mode)),
    capabilityRevisionId: line.supplier_capability_revision_id ?? "",
    capabilitySummary: "",
    qualificationStatus: line.supplier_capability_revision_id
      ? ("VALID" as const)
      : ("INVALID" as const),
  }))
}

async function projectTask(
  workItem: BackendWorkItem,
  profile: BackendAccountProfile
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

  const submissionLines =
    submission?.lines?.map((line) => ({
      submissionLineId: line.id,
      itemName: line.item_name_snapshot ?? `行 ${line.line_no}`,
      itemSku: line.sku_id ?? "",
      specification: line.spec_snapshot ?? undefined,
      committedQuantity: String(line.quantity ?? "0"),
      unit: line.base_unit_code ?? line.unit_snapshot ?? "",
      requestedDeliveryDate:
        secsToIso(line.fulfillment_due_at).slice(0, 10) || "—",
      unitPriceGross: line.unit_price_gross ?? undefined,
      fulfillmentMode: line.fulfillment_mode
        ? mapFulfillmentMode(line.fulfillment_mode)
        : undefined,
      salesTaxRate: line.sales_tax_rate ?? undefined,
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
      workItem.status === "IN_PROGRESS" &&
      workItem.owner_user_id === profile.userid
        ? { claimedByLabel: profile.name }
        : undefined,
    salesSubmission: {
      salesOrderId: detail.sales_order_id,
      salesOrderNo: sales?.order_no ?? "销售单号不可用",
      submissionId: detail.submission_id,
      submissionNo: submission?.submission_no ?? 0,
      subjectHash: detail.submission_id,
      subjectHashSummary: (detail.submission_id ?? "").slice(0, 12),
      submittedAt: secsToIso(submission?.submitted_at) || secsToIso(detail.created_at),
      submittedByLabel:
        submission?.submitted_by === sales?.owner_user_id
          ? (sales?.owner_user_name ?? "销售提交人")
          : "销售提交人",
      customerSnapshot: submission?.customer_name ?? "—",
      contractId: sales?.contract_id ?? undefined,
      contractSnapshot: submission?.contract_no ?? undefined,
      settlementPartySnapshot:
        submission?.settlement_party_name ?? undefined,
      paymentTermLabel: submission?.payment_term_name ?? "—",
      projectName: submission?.project_name ?? undefined,
      businessRemark: submission?.business_remark ?? undefined,
      grossAmount: String(
        submission?.gross_amount ??
          sales?.working_copy?.gross_amount ??
          "0"
      ),
      netAmount: submission?.net_amount,
      taxAmount: submission?.tax_amount,
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
  const profile = await fetchAccountProfile()
  const workItems = await fetchWorkItemsForQueue(filters, profile)

  const projected = (
    await Promise.all(workItems.map((wi) => projectTask(wi, profile)))
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
  const [detail, profile] = await Promise.all([
    apiGet<BackendWorkItem>(
      `/admin/work-items/${encodeURIComponent(workItemId)}`
    ),
    fetchAccountProfile(),
  ])
  const claimed = await apiPost<BackendWorkItem>(
    `/admin/work-items/${encodeURIComponent(workItemId)}/claim`,
    { version: detail.version }
  )
  return {
    workItemId,
    claimedByLabel:
      claimed.owner_user_id === profile.userid ? profile.name : "当前账号",
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
      supplier_offering_revision_id: line.offeringRevisionId,
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
        purchaseOrders: (data.purchase_orders ?? []).map((order) => ({
          purchaseOrderId: order.purchase_order_id,
          purchaseNo: order.purchase_no,
        })),
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
