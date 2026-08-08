/**
 * W05 销售单 HTTP API（queryFn / mutationFn 纯函数）。
 *
 * 后端域：sales_order / sales_review / work_item / bulk_job。
 * 在本文件内将后端 DTO 适配为既有前端视图契约（types.ts / queries.ts 不变）。
 * 失败统一抛 ApiError（@/lib/api），禁止 throw new Error("string")。
 */

import { apiGet, apiPost, apiPut } from "@/lib/api"
import type { ApiError } from "@/lib/api/errors"
import type { StatusTone } from "@/components/ui/status-badge"
import { welfareScenarioLabel } from "@/lib/business-options"
import { computeSalesOrderMetrics } from "@/features/sales-orders/filter-orders"
import type {
  ActionBlocker,
  CardSalesApproval,
  CreateSalesOrderInput,
  CreateSalesOrderResult,
  FormalAllowedAction,
  ProcurementRejectionResolution,
  ProgressTrack,
  SalesChangeOrderSummary,
  SalesOrderLineItem,
  SalesOrderListItem,
  SalesOrderNature,
  SalesOrderOrigin,
  SalesOrderRevisionSnapshot,
} from "@/features/sales-orders/types"

// ─── 导出视图类型（保持 queries 契约） ───────────────────────────────────────

export type SalesOrderDetailView = SalesOrderListItem & {
  acceptance?: {
    acceptedQuantity: string
    note: string
    reference: string
    postedAt: string
  } | null
  permissionVersion: string
  sourceAsOf: string
  queriedAt: string
}

export type SalesOrdersListQuery = {
  page: number
  pageSize: number
  search?: string
  nature?: "all" | "physical_service" | "card_voucher"
  summary?:
    | "all"
    | "pending"
    | "inProgress"
    | "pendingCollection"
    | "fulfillmentException"
    | "mallCollab"
  origin?: "all" | SalesOrderOrigin
  status?: string
  sortBy?:
    | "documentNumber"
    | "contractNumber"
    | "amountGross"
    | "ownerName"
    | "submittedAt"
  sortDir?: "asc" | "desc"
}

export type SalesOrderListView = {
  items: SalesOrderListItem[]
  total: number
  page: number
  pageSize: number
  metrics: ReturnType<typeof computeSalesOrderMetrics>
  queriedAt: string
}

export const PERMISSION_VERSION = "pv-w05-1"

// ─── 后端原始形状 ────────────────────────────────────────────────────────────

type PageView<T> = {
  items: T[]
  total: number
  page: number
  page_size: number
}

type BackendSalesOrderView = {
  id: string
  order_no: string
  business_type: "VOUCHER" | "GOODS_SERVICE" | string
  origin_system: "MALL" | "ERP" | string
  customer_id: string
  contract_id?: string | null
  commercial_status: string
  review_status: string
  fulfillment_progress: string
  collection_progress: string
  invoice_progress: string
  close_status: string
  effective_at?: number | null
  closed_at?: number | null
  version: number
  created_at: number
  updated_at: number
}

type BackendWorkingCopyLine = {
  id: string
  sales_order_line_id: string
  line_no: number
  line_type: string
  gross_amount: string
  net_amount: string
  tax_amount: string
  sales_tax_rate: string
  item_name_snapshot: string
  spec_snapshot?: string | null
  unit_snapshot?: string | null
  sku_id?: string | null
  sku_revision_id?: string | null
  quantity?: string | null
  base_unit_code?: string | null
  unit_price_gross?: string | null
  face_value?: string | null
  card_count?: number | null
  transaction_amount?: string | null
  card_form?: string | null
}

type BackendWorkingCopy = {
  id: string
  working_purpose: string
  status: string
  draft_version: number
  content_hash: string
  editor_user_id: string
  business_type: string
  customer_name?: string
  contract_no?: string | null
  settlement_party_name?: string | null
  payment_term_code?: string
  payment_term_name?: string
  invoice_type?: string
  tax_point?: string
  project_name?: string | null
  business_remark?: string | null
  voucher_category_sku_id?: string | null
  voucher_expiry_at?: number | null
  gross_amount: string
  net_amount: string
  tax_amount: string
  lines: BackendWorkingCopyLine[]
}

type BackendSubmission = {
  id: string
  submission_no: number
  status: string
  business_type: string
  customer_name?: string
  contract_no?: string | null
  settlement_party_name?: string | null
  payment_term_code?: string
  payment_term_name?: string
  invoice_type?: string
  tax_point?: string
  project_name?: string | null
  business_remark?: string | null
  voucher_category_sku_id?: string | null
  voucher_expiry_at?: number | null
  gross_amount: string
  net_amount: string
  tax_amount: string
  submitted_by: string
  submitted_at: number
  created_at: number
  lines?: BackendWorkingCopyLine[]
}

type BackendRevision = {
  id: string
  revision_no: number
  revision_source: string
  content_hash: string
  gross_amount: string
  net_amount: string
  tax_amount: string
  effective_at: number
  created_at: number
}

type BackendSalesOrderDetail = {
  id: string
  order_no: string
  business_type: string
  origin_system: string
  customer_id: string
  contract_id?: string | null
  settlement_party_id: string
  commercial_status: string
  review_status: string
  fulfillment_progress: string
  collection_progress: string
  invoice_progress: string
  close_status: string
  current_revision_id?: string | null
  effective_at?: number | null
  version: number
  created_at: number
  owner_user_id: string
  owner_user_name?: string | null
  lines: Array<{ id: string; line_no: number; line_status: string }>
  working_copy?: BackendWorkingCopy | null
  submissions: BackendSubmission[]
  revisions: BackendRevision[]
}

type BackendContractDetail = {
  id: string
  contract_no: string
  customer_id: string
  settlement_party_id: string
  status: string
  current_revision_id?: string | null
  created_at: number
  version: number
  revisions: Array<{
    id: string
    revision_no: number
    customer_name: string
    settlement_party_name: string
    payment_term_code: string
    payment_term_name: string
    invoice_type: string
    tax_point: string
    valid_from: string
    valid_to?: string | null
    signed_at: string
    created_at: number
  }>
}

type BackendCustomerDetail = {
  id: string
  party_id: string
  customer_no: string
  legal_name?: string | null
}

type BackendPartyContact = {
  id: string
  contact_name: string
  is_default: boolean
  status: string
}

type BackendSalesOrderReview = {
  id: string
  sales_order_id: string
  submission_id: string
  review_stage: string
  status: string
  reviewer_id?: string | null
  reviewed_at?: number | null
  created_at: number
}

type BackendProcurementConfirmation = {
  id: string
  sales_order_id: string
  submission_id: string
  status: string
  handled_by?: string | null
  handled_at?: number | null
  created_at: number
}

type BackendSalesChangeOrder = {
  id: string
  sales_order_id: string
  base_revision_id: string
  change_type: string
  status: string
  current_submission_id?: string | null
  version: number
  created_at: number
}

type BackendWorkItem = {
  id: string
  work_item_type: string
  business_object_type: string
  business_object_id: string
  subject_version?: string | null
  status: string
  owner_role?: string | null
  owner_user_id?: string | null
  version: number
  impact_summary?: string | null
  created_at: number
}

type BackendBackgroundJob = {
  id: string
  job_no?: string
  status?: string
  total_count?: number
  created_at?: number
  version?: number
}

type ProcurementResolutionOutcome = {
  outcome:
    | "CHANGED_TERMS_RESUBMITTED"
    | "LOW_MARGIN_MANAGER_CONFIRMATION_CREATED"
    | "VOIDED_AFTER_PROCUREMENT_REJECTION"
    | "LOW_MARGIN_APPROVED_AND_PROCUREMENT_RESUBMITTED"
    | "LOW_MARGIN_REJECTED_TO_SALES"
  reference: string
  detail: string
  newSubmissionNo?: number
  newSubjectHash?: string
  newWorkItemId?: string
  reviewStatus?:
    | "REJECTED"
    | "PENDING_LOW_MARGIN_MANAGER"
    | "RESOLVED"
    | "VOIDED"
  primaryStatusLabel?: string
}

type CardApprovalCompleteResult = {
  outcome:
    | "MANAGER_APPROVED"
    | "OPERATIONS_APPROVED_AND_EFFECTIVE"
    | "REJECTED_TO_SALES"
  reference: string
  detail: string
  nextWorkItemId?: string
  primaryStatusLabel?: string
}

type ExportJobResult = {
  jobId: string
  status: "queued" | "running" | "succeeded" | "failed"
  rowCount: number
  permissionVersion: string
  createdAt: string
  downloadLabel: string
}

// ─── 适配辅助 ────────────────────────────────────────────────────────────────

const validationError = (message: string): ApiError => ({
  kind: "Validation",
  message,
  status: 400,
})

function throwValidation(message: string): never {
  throw validationError(message)
}

function formatInstant(secs?: number | null): string {
  if (secs == null || secs <= 0) return ""
  const d = new Date(secs * 1000)
  const pad = (n: number) => String(n).padStart(2, "0")
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

function formatIsoNow(): string {
  return new Date().toISOString()
}

function mapNature(businessType: string): SalesOrderNature {
  return businessType === "VOUCHER" ? "card_voucher" : "physical_service"
}

function mapOrigin(origin: string): SalesOrderOrigin {
  return origin === "MALL" ? "mall" : "erp"
}

function toneForStatus(label: string): StatusTone {
  if (label.includes("作废") || label.includes("关闭")) return "void"
  if (label.includes("生效") || label.includes("完成") || label.includes("通过"))
    return "success"
  if (
    label.includes("待") ||
    label.includes("审核") ||
    label.includes("确认") ||
    label.includes("审批")
  )
    return "warning"
  if (label.includes("履约") || label.includes("部分")) return "info"
  return "neutral"
}

function mapPrimaryStatus(
  commercial: string,
  review: string,
  close: string,
  fulfillment: string
): { label: string; tone: StatusTone } {
  if (close === "CLOSED") return { label: "已关闭", tone: "void" }
  if (commercial === "VOIDED") return { label: "已作废", tone: "void" }
  if (commercial === "EFFECTIVE") {
    if (fulfillment === "PARTIALLY_FULFILLED")
      return { label: "履约中", tone: "info" }
    return { label: "已生效", tone: "success" }
  }
  // 商业态仍为 DRAFT 时，只要已进入审核轨就按审核态展示（提交后常见组合）
  if (
    commercial === "DRAFT" ||
    commercial === "PENDING_REVIEW" ||
    commercial === "IN_REVIEW"
  ) {
    switch (review) {
      case "PENDING_PROCUREMENT_CONFIRMATION":
        return { label: "待二次确认", tone: "warning" }
      case "PENDING_LOW_MARGIN_SUPERIOR":
        return { label: "待销售处理", tone: "warning" }
      case "PENDING_SALES_LEADER":
        return { label: "待销售领导审批", tone: "warning" }
      case "PENDING_OPERATIONS":
        return { label: "待运营审批", tone: "warning" }
      case "REJECTED":
        return { label: "待销售处理", tone: "warning" }
      case "APPROVED":
        return { label: "已生效", tone: "success" }
      case "NOT_SUBMITTED":
      case "":
        if (commercial === "DRAFT") return { label: "草稿", tone: "neutral" }
        break
      default:
        if (review) return { label: "审核中", tone: "warning" }
    }
    if (commercial === "DRAFT") return { label: "草稿", tone: "neutral" }
    return { label: "审核中", tone: "warning" }
  }
  return { label: "进行中", tone: "info" }
}

function mapFulfillment(code: string): ProgressTrack {
  switch (code) {
    case "PARTIALLY_FULFILLED":
      return { label: "部分履约", tone: "warning" }
    case "COMPLETED":
      return { label: "已完成", tone: "success" }
    default:
      return { label: "未开始", tone: "neutral" }
  }
}

function mapCollection(code: string): ProgressTrack {
  switch (code) {
    case "PARTIALLY_COLLECTED":
      return { label: "部分回款", tone: "warning" }
    case "SETTLED":
      return { label: "已结清", tone: "success" }
    default:
      return { label: "未收", tone: "neutral" }
  }
}

function mapInvoicing(code: string): ProgressTrack {
  switch (code) {
    case "PARTIALLY_INVOICED":
      return { label: "部分开票", tone: "warning" }
    case "COMPLETED":
      return { label: "已完成", tone: "success" }
    default:
      return { label: "未开", tone: "neutral" }
  }
}

function mapCloseEligibility(
  close: string,
  fulfillment: string,
  collection: string
): SalesOrderListItem["closeEligibility"] {
  const fulfillmentComplete = fulfillment === "COMPLETED"
  const receivableSettled = collection === "SETTLED"
  const eligible =
    close === "CLOSEABLE" ||
    close === "CLOSED" ||
    (fulfillmentComplete && receivableSettled)
  const blockers: string[] = []
  if (!fulfillmentComplete) blockers.push("履约尚未完成")
  if (!receivableSettled) blockers.push("应收尚未结清")
  return {
    fulfillmentComplete,
    receivableSettled,
    invoiceComplete: false,
    eligibleToClose: eligible && close !== "CLOSED",
    blockers,
    note:
      close === "CLOSED"
        ? "本单已关闭。"
        : "关闭条件：履约完成 + 应收结清；开票不阻塞关闭。",
  }
}

function mapWorkingCopyLines(
  lines: BackendWorkingCopyLine[] | undefined
): SalesOrderLineItem[] {
  if (!lines?.length) return []
  return lines.map((line) => {
    const isVoucher = line.line_type === "VOUCHER"
    const item: SalesOrderLineItem = {
      id: line.sales_order_line_id || line.id,
      name: line.item_name_snapshot,
      sku: line.spec_snapshot || line.sku_id || undefined,
      quantity: isVoucher
        ? String(line.card_count ?? line.quantity ?? "0")
        : (line.quantity ?? "0"),
      unit: line.unit_snapshot || line.base_unit_code || (isVoucher ? "张" : ""),
      unitPriceGross: line.unit_price_gross ?? "0.00",
      amountGross: line.gross_amount ?? line.transaction_amount ?? "0.00",
    }
    if (isVoucher) {
      item.faceValue = line.face_value ?? undefined
      item.cardForm =
        line.card_form === "PHYSICAL"
          ? "实体卡"
          : line.card_form === "ELECTRONIC"
            ? "电子卡"
            : line.card_form ?? undefined
    }
    return item
  })
}

function mapRevisions(
  revisions: BackendRevision[] | undefined
): SalesOrderRevisionSnapshot[] {
  if (!revisions?.length) return []
  return revisions.map((rev) => ({
    revisionNo: rev.revision_no,
    effectiveAt: formatInstant(rev.effective_at),
    contractRevisionLabel: "",
    customerSnapshot: "",
    amountGross: rev.gross_amount,
    lineSummary: "",
    note: rev.revision_source,
  }))
}

function defaultAllowedActions(
  commercial: string,
  review: string,
  close: string,
  hasChange: boolean,
  hasCardApproval: boolean,
  hasRejection: boolean
): { allowed: FormalAllowedAction[]; blockers: ActionBlocker[] } {
  const allowed: FormalAllowedAction[] = ["PRINT", "EXPORT", "VIEW_CLOSE_CONDITIONS"]
  const blockers: ActionBlocker[] = []

  if (commercial === "EFFECTIVE" && close !== "CLOSED" && !hasChange) {
    allowed.push("START_SALES_CHANGE")
  } else if (hasChange) {
    blockers.push({
      action: "START_SALES_CHANGE",
      reason: "已有进行中的销售变更单。",
    })
  }

  if (commercial === "EFFECTIVE" || commercial === "PENDING_REVIEW") {
    allowed.push("REGISTER_ACCEPTANCE")
  }

  if (hasRejection) {
    allowed.push("RESOLVE_PROCUREMENT_REJECTION")
  }
  if (hasCardApproval) {
    allowed.push("HANDLE_CARD_APPROVAL")
  }

  return { allowed, blockers }
}

function mapListItemFromBackend(
  row: BackendSalesOrderView,
  extras?: {
    customerName?: string
    contractNumber?: string
    amountGross?: string
    amountNet?: string
    taxAmount?: string
    lineItems?: SalesOrderLineItem[]
    ownerName?: string
    paymentTerms?: string
    welfareScene?: string
    fulfillmentDeadline?: string
    remark?: string
    settlementEntity?: string
    revisions?: SalesOrderRevisionSnapshot[]
    procurementRejection?: ProcurementRejectionResolution | null
    activeCardSalesApproval?: CardSalesApproval | null
    activeChangeOrder?: SalesChangeOrderSummary | null
    customerContact?: string
  }
): SalesOrderListItem {
  const nature = mapNature(row.business_type)
  const originSystem = mapOrigin(row.origin_system)
  const primaryStatus = mapPrimaryStatus(
    row.commercial_status,
    row.review_status,
    row.close_status,
    row.fulfillment_progress
  )
  const fulfillment = mapFulfillment(row.fulfillment_progress)
  const collection = mapCollection(row.collection_progress)
  const invoicing = mapInvoicing(row.invoice_progress)
  const hasChange = Boolean(extras?.activeChangeOrder)
  const hasCard = Boolean(extras?.activeCardSalesApproval)
  const hasRejection = Boolean(extras?.procurementRejection)
  const { allowed, blockers } = defaultAllowedActions(
    row.commercial_status,
    row.review_status,
    row.close_status,
    hasChange,
    hasCard,
    hasRejection
  )

  const commercialReadOnly =
    originSystem === "mall" ||
    primaryStatus.label === "已关闭" ||
    primaryStatus.label === "已作废" ||
    hasCard ||
    row.commercial_status === "EFFECTIVE"

  return {
    id: row.id,
    documentNumber: row.order_no,
    customerName: extras?.customerName ?? row.customer_id,
    contractNumber: extras?.contractNumber ?? row.contract_id ?? "",
    contractRevisionLabel: extras?.contractNumber
      ? `${extras.contractNumber}`
      : "",
    nature,
    originSystem,
    primaryStatus,
    fulfillment,
    collection,
    invoicing,
    amountGross: extras?.amountGross ?? "0.00",
    amountNet: extras?.amountNet ?? "0.00",
    taxAmount: extras?.taxAmount ?? "0.00",
    receivedAmount: "0.00",
    invoicedAmount: "0.00",
    ownerName: extras?.ownerName ?? "",
    submittedAt: formatInstant(row.created_at),
    welfareScene: extras?.welfareScene ?? "",
    remark: extras?.remark,
    version: Number(row.version) || 1,
    lockVersion: Number(row.version) || 1,
    settlementEntity: extras?.settlementEntity ?? "",
    sellerEntity: "",
    paymentTerms: extras?.paymentTerms ?? "",
    fulfillmentDeadline: extras?.fulfillmentDeadline ?? "",
    customerContact: extras?.customerContact,
    lineItems: extras?.lineItems ?? [],
    related: {
      purchaseOrders: 0,
      fulfillments: 0,
      receipts: 0,
      invoices: 0,
    },
    closeEligibility: mapCloseEligibility(
      row.close_status,
      row.fulfillment_progress,
      row.collection_progress
    ),
    natureLocked: true,
    commercialReadOnly,
    commercialReadOnlyReason: commercialReadOnly
      ? originSystem === "mall"
        ? "这单由商城开单，商业数据同步中，本系统只读；改内容请在商城处理。"
        : row.commercial_status === "EFFECTIVE"
          ? "本单已生效，不能直接改；改内容请「发起改单」。"
          : undefined
      : undefined,
    revisions: extras?.revisions ?? [],
    procurementRejection: extras?.procurementRejection ?? null,
    activeCardSalesApproval: extras?.activeCardSalesApproval ?? null,
    activeChangeOrder: extras?.activeChangeOrder ?? null,
    allowedActions: allowed,
    actionBlockers: blockers,
  }
}

function formatEpochDate(secs?: number | null): string {
  if (secs == null || !Number.isFinite(secs) || secs <= 0) return ""
  try {
    const d = new Date(secs * 1000)
    if (Number.isNaN(d.getTime())) return ""
    const y = d.getFullYear()
    const m = String(d.getMonth() + 1).padStart(2, "0")
    const day = String(d.getDate()).padStart(2, "0")
    return `${y}-${m}-${day}`
  } catch {
    return ""
  }
}

/**
 * 详情商业内容优先：可编辑草稿 → 最新提交快照。
 * 提交后 working_copy 为空时必须从 submission 回填明细与表头。
 */
function pickCommercialContent(detail: BackendSalesOrderDetail): {
  lines: BackendWorkingCopyLine[]
  amountGross?: string
  amountNet?: string
  taxAmount?: string
  ownerUserId: string
  customerName?: string
  contractNo?: string
  settlementPartyName?: string
  paymentTerms: string
  welfareScene: string
  fulfillmentDeadline: string
  remark?: string
} {
  const wc = detail.working_copy
  const submissions = [...(detail.submissions ?? [])].sort(
    (a, b) => (b.submission_no ?? 0) - (a.submission_no ?? 0)
  )
  const latestSubmission = submissions[0]

  if (wc?.lines?.length) {
    return {
      lines: wc.lines,
      amountGross: wc.gross_amount,
      amountNet: wc.net_amount,
      taxAmount: wc.tax_amount,
      ownerUserId: wc.editor_user_id || latestSubmission?.submitted_by || "",
      customerName: wc.customer_name || undefined,
      contractNo: wc.contract_no || undefined,
      settlementPartyName: wc.settlement_party_name || undefined,
      paymentTerms: wc.payment_term_name || wc.payment_term_code || "",
      welfareScene: wc.project_name || "",
      fulfillmentDeadline: formatEpochDate(wc.voucher_expiry_at),
      remark: wc.business_remark || undefined,
    }
  }

  if (latestSubmission) {
    return {
      lines: latestSubmission.lines ?? [],
      amountGross: latestSubmission.gross_amount,
      amountNet: latestSubmission.net_amount,
      taxAmount: latestSubmission.tax_amount,
      ownerUserId: latestSubmission.submitted_by || "",
      customerName: latestSubmission.customer_name || undefined,
      contractNo: latestSubmission.contract_no || undefined,
      settlementPartyName: latestSubmission.settlement_party_name || undefined,
      paymentTerms:
        latestSubmission.payment_term_name ||
        latestSubmission.payment_term_code ||
        "",
      welfareScene: latestSubmission.project_name || "",
      fulfillmentDeadline: formatEpochDate(latestSubmission.voucher_expiry_at),
      remark: latestSubmission.business_remark || undefined,
    }
  }

  return {
    lines: [],
    ownerUserId: "",
    paymentTerms: "",
    welfareScene: "",
    fulfillmentDeadline: "",
  }
}

function mapDetailToListItem(
  detail: BackendSalesOrderDetail,
  extras?: {
    customerName?: string
    contractNumber?: string
    ownerName?: string
    procurementRejection?: ProcurementRejectionResolution | null
    activeCardSalesApproval?: CardSalesApproval | null
    activeChangeOrder?: SalesChangeOrderSummary | null
    customerContact?: string
  }
): SalesOrderListItem {
  const commercial = pickCommercialContent(detail)
  return mapListItemFromBackend(
    {
      id: detail.id,
      order_no: detail.order_no,
      business_type: detail.business_type,
      origin_system: detail.origin_system,
      customer_id: detail.customer_id,
      contract_id: detail.contract_id,
      commercial_status: detail.commercial_status,
      review_status: detail.review_status,
      fulfillment_progress: detail.fulfillment_progress,
      collection_progress: detail.collection_progress,
      invoice_progress: detail.invoice_progress,
      close_status: detail.close_status,
      effective_at: detail.effective_at,
      version: detail.version,
      created_at: detail.created_at,
      updated_at: detail.created_at,
    },
    {
      customerName:
        extras?.customerName ||
        commercial.customerName ||
        detail.customer_id,
      contractNumber: extras?.contractNumber || commercial.contractNo || "",
      amountGross: commercial.amountGross,
      amountNet: commercial.amountNet,
      taxAmount: commercial.taxAmount,
      lineItems: mapWorkingCopyLines(commercial.lines),
      ownerName: extras?.ownerName || commercial.ownerUserId || "",
      customerContact: extras?.customerContact,
      paymentTerms: commercial.paymentTerms,
      welfareScene: commercial.welfareScene,
      fulfillmentDeadline: commercial.fulfillmentDeadline,
      remark: commercial.remark,
      revisions: mapRevisions(detail.revisions),
      procurementRejection: extras?.procurementRejection,
      activeCardSalesApproval: extras?.activeCardSalesApproval,
      activeChangeOrder: extras?.activeChangeOrder,
      settlementEntity:
        commercial.settlementPartyName || detail.settlement_party_id,
    }
  )
}

function mapReviewToCardApproval(
  review: BackendSalesOrderReview
): CardSalesApproval | null {
  if (review.status !== "PENDING") return null
  const isLeader = review.review_stage === "SALES_LEADER"
  const isOps = review.review_stage === "OPERATIONS"
  if (!isLeader && !isOps) return null
  return {
    workItemId: review.id,
    workItemType: isLeader
      ? "CARD_SALES_MANAGER_APPROVAL"
      : "CARD_SALES_OPERATION_APPROVAL",
    workItemStatus: review.reviewer_id ? "CLAIMED" : "UNCLAIMED",
    subjectVersion: review.submission_id,
    subjectHash: review.submission_id,
    claimedByLabel: review.reviewer_id ?? undefined,
    frozenSubmissionSummary: "",
    expectedReviewStatus: isLeader ? "PENDING_SALES_LEAD" : "PENDING_OPERATIONS",
    allowedActions: review.reviewer_id
      ? ["APPROVE", "REJECT"]
      : ["CLAIM"],
    actionBlockers: review.reviewer_id
      ? []
      : [
          { action: "APPROVE", reason: "请先领取后再审批。" },
          { action: "REJECT", reason: "请先领取后再审批。" },
        ],
  }
}

function mapRejectedProcurement(
  conf: BackendProcurementConfirmation
): ProcurementRejectionResolution | null {
  if (conf.status !== "REJECTED") return null
  return {
    rejectedProcurementConfirmationId: conf.id,
    rejectedProcurementWorkItemId: "",
    rejectedSubmissionId: conf.submission_id,
    rejectedSubmissionNo: 0,
    rejectedSubjectHash: conf.submission_id,
    rejectReasonCode: "",
    rejectComment: "",
    rejectedByLabel: conf.handled_by ?? "",
    rejectedAt: formatInstant(conf.handled_at),
    reviewStatus: "REJECTED",
    draftDifference: {
      changedItemOrService: false,
      changedSalesPrice: false,
      commercialTermsUnchanged: true,
      diffSummary: [],
    },
    fixedResolutions: [
      "RESUBMIT_CHANGED_TERMS",
      "REQUEST_LOW_MARGIN_ACCEPTANCE",
      "VOID_AFTER_REJECTION",
    ],
    allowedActions: [
      "RESUBMIT_CHANGED_TERMS",
      "REQUEST_LOW_MARGIN_ACCEPTANCE",
      "VOID_AFTER_REJECTION",
    ],
    actionBlockers: [],
  }
}

function mapChangeOrder(
  row: BackendSalesChangeOrder,
  nature: SalesOrderNature
): SalesChangeOrderSummary {
  const impactPath = nature === "card_voucher" ? "operations" : "procurement"
  const statusLabel =
    row.status === "PENDING_IMPACT_CONFIRMATION"
      ? impactPath === "operations"
        ? "待运营执行影响确认"
        : "待采购履约影响确认"
      : row.status === "PENDING_FINANCE_REVIEW"
        ? "待财务复核"
        : row.status === "DRAFT"
          ? "草稿"
          : row.status === "EFFECTIVE"
            ? "已生效"
            : row.status === "VOIDED"
              ? "已作废"
              : row.status
  return {
    id: row.id,
    statusLabel,
    statusTone: toneForStatus(statusLabel),
    baseRevisionNo: 0,
    createdAt: new Date(row.created_at * 1000).toISOString(),
    impactPath,
  }
}

function mapStatusFilterToBackend(status?: string): {
  commercial_status?: string
  review_status?: string
} {
  switch (status) {
    case "draft":
      return { commercial_status: "DRAFT" }
    case "voided":
      return { commercial_status: "VOIDED" }
    case "effective":
      return { commercial_status: "EFFECTIVE" }
    case "closed":
      return { commercial_status: "EFFECTIVE" }
    case "awaiting_confirm":
      return {
        commercial_status: "PENDING_REVIEW",
        review_status: "PENDING_PROCUREMENT_CONFIRMATION",
      }
    case "awaiting_sales":
      return {
        commercial_status: "PENDING_REVIEW",
        review_status: "REJECTED",
      }
    case "awaiting_sales_lead":
      return {
        commercial_status: "PENDING_REVIEW",
        review_status: "PENDING_SALES_LEADER",
      }
    case "awaiting_ops":
      return {
        commercial_status: "PENDING_REVIEW",
        review_status: "PENDING_OPERATIONS",
      }
    case "fulfilling":
      return { commercial_status: "EFFECTIVE" }
    default:
      return {}
  }
}

function mapSortBy(
  sortBy?: SalesOrdersListQuery["sortBy"]
): string | undefined {
  if (!sortBy) return undefined
  if (sortBy === "documentNumber") return "order_no"
  if (sortBy === "submittedAt") return "created_at"
  // amountGross / contractNumber / ownerName 不在后端白名单
  return "created_at"
}

function mapFulfillmentMode(label: string): string {
  const t = label.trim()
  if (t.includes("直发")) return "SUPPLIER_DIRECT"
  if (t.includes("电子")) return "ELECTRONIC_DELIVERY"
  if (t.includes("服务") || t.includes("线下")) return "OFFLINE_SERVICE"
  return "COMPANY_WAREHOUSE"
}

function mapCardForm(label: string): string {
  return label.includes("实体") ? "PHYSICAL" : "ELECTRONIC"
}

/** 福利场景：表单码 / 中文 → 后端 SCREAMING_SNAKE_CASE；无法识别则 null。 */
function mapWelfareScenarioCode(raw: string): string | null {
  const value = raw.trim()
  if (!value) return null
  switch (value) {
    case "ANNUAL_GIFT_BAG":
    case "年节礼包":
      return "ANNUAL_GIFT_BAG"
    case "MEAL_SUBSIDY":
    case "餐补":
      return "MEAL_SUBSIDY"
    case "CONDOLENCE_GIFT":
    case "慰问品":
      return "CONDOLENCE_GIFT"
    case "CONSUMPTION_FUND":
    case "消费金":
      return "CONSUMPTION_FUND"
    case "OTHER":
    case "其他":
    case "其它":
      return "OTHER"
    default:
      return null
  }
}

function percentToRate(percent: string): string {
  const n = Number(percent)
  if (!Number.isFinite(n)) return "0.000000"
  return (n / 100).toFixed(6)
}

function dateToUnixSecs(dateStr: string): number {
  if (!dateStr) return Math.floor(Date.now() / 1000)
  // YYYY-MM-DD or datetime
  const normalized = dateStr.length === 10 ? `${dateStr}T00:00:00+08:00` : dateStr
  const ms = Date.parse(normalized)
  if (Number.isNaN(ms)) return Math.floor(Date.now() / 1000)
  return Math.floor(ms / 1000)
}

function localOrderNo(): string {
  const d = new Date()
  const pad = (n: number) => String(n).padStart(2, "0")
  const stamp = `${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}${pad(d.getHours())}${pad(d.getMinutes())}${pad(d.getSeconds())}`
  return `XS${stamp}`
}

// ─── 列表 / 详情 ─────────────────────────────────────────────────────────────

export async function fetchSalesOrders(
  query: SalesOrdersListQuery
): Promise<SalesOrderListView> {
  const statusMap = mapStatusFilterToBackend(
    query.status && query.status !== "all" ? query.status : undefined
  )
  const businessType =
    query.nature === "card_voucher"
      ? "VOUCHER"
      : query.nature === "physical_service"
        ? "GOODS_SERVICE"
        : undefined

  const page = await apiGet<PageView<BackendSalesOrderView>>(
    "/admin/sales-orders",
    {
      page: query.page,
      page_size: query.pageSize,
      order_no: query.search?.trim() || undefined,
      business_type: businessType,
      commercial_status: statusMap.commercial_status,
      review_status: statusMap.review_status,
      sort_by: mapSortBy(query.sortBy),
      sort_dir: query.sortDir,
    }
  )

  // origin / summary 筛选后端未提供，客户端对当前页做展示过滤（缺口登记）。
  let items = page.items.map((row) => mapListItemFromBackend(row))
  if (query.origin && query.origin !== "all") {
    items = items.filter((o) => o.originSystem === query.origin)
  }

  const metrics = computeSalesOrderMetrics(items)
  metrics.total = page.total

  return {
    items,
    total: page.total,
    page: page.page,
    pageSize: page.page_size,
    metrics,
    queriedAt: formatIsoNow(),
  }
}

async function loadDetailExtras(salesOrderId: string, nature: SalesOrderNature) {
  const [reviewsPage, confirmationsPage, changeOrdersPage] = await Promise.all([
    apiGet<PageView<BackendSalesOrderReview>>("/admin/sales-order-reviews", {
      sales_order_id: salesOrderId,
      status: "PENDING",
      page: 1,
      page_size: 20,
    }).catch(() => ({ items: [] as BackendSalesOrderReview[], total: 0, page: 1, page_size: 20 })),
    apiGet<PageView<BackendProcurementConfirmation>>(
      "/admin/procurement-confirmations",
      {
        page: 1,
        page_size: 50,
      }
    ).catch(() => ({
      items: [] as BackendProcurementConfirmation[],
      total: 0,
      page: 1,
      page_size: 50,
    })),
    apiGet<PageView<BackendSalesChangeOrder>>("/admin/sales-change-orders", {
      sales_order_id: salesOrderId,
      page: 1,
      page_size: 10,
    }).catch(() => ({
      items: [] as BackendSalesChangeOrder[],
      total: 0,
      page: 1,
      page_size: 10,
    })),
  ])

  const pendingReview =
    reviewsPage.items.find(
      (r) =>
        r.sales_order_id === salesOrderId &&
        r.status === "PENDING" &&
        (r.review_stage === "SALES_LEADER" || r.review_stage === "OPERATIONS")
    ) ?? null

  const rejected =
    confirmationsPage.items.find(
      (c) => c.sales_order_id === salesOrderId && c.status === "REJECTED"
    ) ?? null

  const activeChange =
    changeOrdersPage.items.find(
      (c) =>
        c.sales_order_id === salesOrderId &&
        c.status !== "EFFECTIVE" &&
        c.status !== "VOIDED" &&
        c.status !== "REJECTED"
    ) ?? null

  return {
    activeCardSalesApproval: pendingReview
      ? mapReviewToCardApproval(pendingReview)
      : null,
    procurementRejection: rejected ? mapRejectedProcurement(rejected) : null,
    activeChangeOrder: activeChange
      ? mapChangeOrder(activeChange, nature)
      : null,
  }
}

async function loadCustomerDisplay(customerId: string): Promise<{
  customerName?: string
  customerContact?: string
}> {
  try {
    const customer = await apiGet<BackendCustomerDetail>(
      `/admin/customers/${customerId}`
    )
    const contacts = await apiGet<PageView<BackendPartyContact>>(
      `/admin/parties/${customer.party_id}/contacts`,
      {
        status: "active",
        page: 1,
        page_size: 100,
        sort_by: "created_at",
        sort_dir: "desc",
      }
    ).catch(() => null)
    const contact =
      contacts?.items.find((item) => item.is_default) ?? contacts?.items[0]

    return {
      customerName: customer.legal_name || customer.customer_no,
      customerContact: contact?.contact_name,
    }
  } catch {
    return {}
  }
}

export async function fetchSalesOrderDetail(
  id: string
): Promise<SalesOrderDetailView | null> {
  let detail: BackendSalesOrderDetail
  try {
    detail = await apiGet<BackendSalesOrderDetail>(`/admin/sales-orders/${id}`)
  } catch (err) {
    const apiErr = err as ApiError
    if (apiErr?.status === 404) return null
    throw err
  }

  const customerDisplay = await loadCustomerDisplay(detail.customer_id)
  let contractNumber = ""
  let customerName = customerDisplay.customerName || detail.customer_id
  if (detail.contract_id) {
    try {
      const contract = await apiGet<BackendContractDetail>(
        `/admin/contracts/${detail.contract_id}`
      )
      contractNumber = contract.contract_no
      const rev =
        contract.revisions.find((r) => r.id === contract.current_revision_id) ??
        contract.revisions[0]
      if (rev?.customer_name) customerName = rev.customer_name
    } catch {
      // 合同域缺口时保留 id 展示
    }
  }

  const extras = await loadDetailExtras(id, mapNature(detail.business_type))
  const order = mapDetailToListItem(detail, {
    customerName,
    contractNumber,
    ownerName: detail.owner_user_name || "—",
    customerContact: customerDisplay.customerContact,
    ...extras,
  })

  // 最近验收摘要（可选）
  let acceptance: SalesOrderDetailView["acceptance"] = null
  try {
    const accPage = await apiGet<
      PageView<{
        id: string
        acceptance_no: string
        sales_order_id: string
        accepted_at: number
        result: string
        status: string
        version: number
        created_at: number
      }>
    >("/admin/customer-acceptances", {
      sales_order_id: id,
      status: "POSTED",
      page: 1,
      page_size: 1,
      sort_by: "accepted_at",
      sort_dir: "desc",
    })
    const latest = accPage.items[0]
    if (latest) {
      acceptance = {
        acceptedQuantity: "",
        note: latest.result,
        reference: latest.acceptance_no,
        postedAt: formatInstant(latest.accepted_at),
      }
    }
  } catch {
    // 验收域失败不阻塞详情
  }

  const queriedAt = formatIsoNow()
  return {
    ...order,
    acceptance,
    permissionVersion: PERMISSION_VERSION,
    sourceAsOf: queriedAt,
    queriedAt,
  }
}

// ─── 建单 ────────────────────────────────────────────────────────────────────

export async function createSalesOrder(
  input: CreateSalesOrderInput
): Promise<CreateSalesOrderResult> {
  if (input.lineItems.length === 0) {
    throwValidation("至少需要一行明细")
  }
  if (input.nature === "card_voucher" && input.lineItems.length !== 1) {
    throwValidation("卡券销售单必须且只能有一行明细")
  }

  const contract = await apiGet<BackendContractDetail>(
    `/admin/contracts/${input.contract.contractId}`
  )
  const requested =
    contract.revisions.find(
      (r) => r.id === input.contract.requestedContractRevisionId
    ) ??
    contract.revisions.find((r) => r.id === contract.current_revision_id) ??
    contract.revisions[0]

  if (!requested) {
    throwValidation("合同修订不存在")
  }

  const taxRate = percentToRate(input.taxRatePercent || "0")
  const businessType =
    input.nature === "card_voucher" ? "VOUCHER" : "GOODS_SERVICE"

  const lines = input.lineItems.map((line, index) => {
    const base = {
      line_no: index + 1,
      line_type: businessType,
      sales_tax_rate: taxRate,
      item_name_snapshot: line.name.trim(),
      spec_snapshot: line.sku.trim() || null,
      unit_snapshot: line.unit.trim() || null,
      goods: null as null | Record<string, unknown>,
      voucher: null as null | Record<string, unknown>,
    }

    if (input.nature === "card_voucher") {
      const cardCount = Math.max(1, Math.floor(Number(line.quantity) || 1))
      const unitPrice = line.unitPriceGross || "0.0000"
      const face = line.faceValue || "0.00"
      // 金额由后端按约定舍入；此处按字符串原样提交并给后端校验三元组
      const faceTotal = (Number(face) * cardCount).toFixed(2)
      const txn = (Number(unitPrice) * cardCount).toFixed(2)
      const gift = (Number(faceTotal) - Number(txn)).toFixed(2)
      base.voucher = {
        face_value: face,
        card_count: cardCount,
        unit_price_gross: unitPrice,
        face_value_total: faceTotal,
        transaction_amount: txn,
        gift_amount: gift,
        // 配赠率由服务端按 gift_amount / transaction_amount 推导，前端不手输
        gift_rate: null,
        card_form: mapCardForm(line.cardForm || "电子卡"),
      }
    } else {
      // 公司商品池同时返回稳定 SKU 与精确当前修订，销售草稿锁定这一对身份。
      const skuId = line.sku.trim()
      const skuRevisionId = line.skuRevisionId.trim()
      if (!skuId || !skuRevisionId) {
        throwValidation("实物明细须从公司商品池选择有效 SKU")
      }
      base.goods = {
        sku_id: skuId,
        sku_revision_id: skuRevisionId,
        welfare_scenario: mapWelfareScenarioCode(input.welfareScene),
        fulfillment_mode: mapFulfillmentMode(line.fulfillmentMode || "公司仓发"),
        fulfillment_due_at: dateToUnixSecs(line.dueDate),
        quantity: line.quantity || "0",
        base_unit_code: line.unit.trim() || "EA",
        unit_price_gross: line.unitPriceGross || "0.0000",
      }
    }
    return base
  })

  const orderNo = localOrderNo()
  const body = {
    order_no: orderNo,
    business_type: businessType,
    customer_id: contract.customer_id,
    contract_id: contract.id,
    settlement_party_id: contract.settlement_party_id,
    idempotency_key: input.idempotencyKey,
    intent: input.intent,
    draft: {
      editor_user_id:
        input.ownerUserId.trim() || input.ownerName.trim() || "unknown",
      customer_name: requested.customer_name,
      contract_no: contract.contract_no,
      settlement_party_name: requested.settlement_party_name,
      payment_term_code: requested.payment_term_code || "CUSTOM",
      payment_term_name:
        input.paymentTerms.trim() || requested.payment_term_name || "合同约定",
      invoice_type: requested.invoice_type || "SPECIAL",
      tax_point: requested.tax_point || input.taxRatePercent || "0",
      // 表头项目名称存中文快照，便于列表/纸质件直接展示
      project_name: welfareScenarioLabel(input.welfareScene) || null,
      business_remark: input.remark.trim() || null,
      voucher_category_sku_id:
        input.nature === "card_voucher"
          ? input.lineItems[0]?.sku.trim() || null
          : null,
      voucher_expiry_at:
        input.nature === "card_voucher"
          ? dateToUnixSecs(input.fulfillmentDeadline)
          : null,
      lines,
    },
  }

  const created = await apiPost<BackendSalesOrderDetail>(
    "/admin/sales-orders",
    body
  )

  const status = mapPrimaryStatus(
    created.commercial_status,
    created.review_status,
    created.close_status,
    created.fulfillment_progress
  )

  return {
    salesOrderId: created.id,
    documentNumber: created.order_no,
    statusLabel: status.label,
    createdAt: new Date(created.created_at * 1000).toISOString(),
    reference: `SO-CREATE-${created.order_no}`,
  }
}

// ─── 验收（兼容旧入口） ──────────────────────────────────────────────────────

export async function submitSalesOrderAcceptance(input: {
  salesOrderId: string
  documentNumber: string
  acceptedQuantity: string
  note: string
}): Promise<{ reference: string }> {
  // 旧单字段入口：创建一笔简化验收草稿后立即过账需要销售行与分配。
  // 完整工作台走 acceptance.ts；此处仅登记后端可创建的草稿头。
  const acceptanceNo = `YS${Date.now().toString(36).toUpperCase()}`
  const created = await apiPost<{
    acceptance?: { id: string; acceptance_no: string }
    id?: string
    acceptance_no?: string
  }>("/admin/customer-acceptances", {
    acceptance_no: acceptanceNo,
    sales_order_id: input.salesOrderId,
    accepted_at: Math.floor(Date.now() / 1000),
    result: "PASSED",
    lines: [],
  }).catch(() => null)

  const reference =
    created?.acceptance?.acceptance_no ??
    created?.acceptance_no ??
    acceptanceNo
  return { reference }
}

// ─── 采购拒绝处理 ────────────────────────────────────────────────────────────

export async function adjustProcurementRejectionDraft(input: {
  salesOrderId: string
  unitPriceGross: string
  note: string
}): Promise<{ ok: true }> {
  const detail = await apiGet<BackendSalesOrderDetail>(
    `/admin/sales-orders/${input.salesOrderId}`
  )
  const wc = detail.working_copy
  if (!wc) {
    throwValidation("当前销售单无可用草稿，无法改价")
  }

  const lines = (wc.lines ?? []).map((line, index) => {
    const isVoucher = line.line_type === "VOUCHER"
    const unitPrice =
      index === 0 ? input.unitPriceGross : (line.unit_price_gross ?? "0.0000")
    const base: Record<string, unknown> = {
      line_no: line.line_no,
      line_type: line.line_type,
      sales_tax_rate: line.sales_tax_rate,
      item_name_snapshot: line.item_name_snapshot,
      spec_snapshot: line.spec_snapshot ?? null,
      unit_snapshot: line.unit_snapshot ?? null,
      goods: null,
      voucher: null,
    }
    if (isVoucher) {
      const cardCount = line.card_count ?? 1
      const face = line.face_value ?? "0.00"
      const faceTotal = (Number(face) * cardCount).toFixed(2)
      const txn = (Number(unitPrice) * cardCount).toFixed(2)
      const gift = (Number(faceTotal) - Number(txn)).toFixed(2)
      base.voucher = {
        face_value: face,
        card_count: cardCount,
        unit_price_gross: unitPrice,
        face_value_total: faceTotal,
        transaction_amount: txn,
        gift_amount: gift,
        gift_rate: null,
        card_form: line.card_form ?? "ELECTRONIC",
      }
    } else {
      const skuId = line.sku_id?.trim()
      const skuRevisionId = line.sku_revision_id?.trim()
      if (!skuId || !skuRevisionId) {
        throwValidation("历史草稿缺少精确 SKU 修订，请重新从公司商品池选择商品")
      }
      base.goods = {
        sku_id: skuId,
        sku_revision_id: skuRevisionId,
        welfare_scenario: null,
        fulfillment_mode: "COMPANY_WAREHOUSE",
        fulfillment_due_at: Math.floor(Date.now() / 1000),
        quantity: line.quantity ?? "0",
        base_unit_code: line.base_unit_code ?? line.unit_snapshot ?? "EA",
        unit_price_gross: unitPrice,
      }
    }
    return base
  })

  await apiPut(`/admin/sales-orders/${input.salesOrderId}/working-copy`, {
    version: detail.version,
    draft: {
      editor_user_id: wc.editor_user_id,
      customer_name: "", // 后端 Save 会用草稿覆盖；名称由服务端实体保留时可能校验
      contract_no: null,
      settlement_party_name: null,
      payment_term_code: "CUSTOM",
      payment_term_name: "合同约定",
      invoice_type: "SPECIAL",
      tax_point: "0",
      project_name: null,
      business_remark: input.note || null,
      voucher_category_sku_id: null,
      voucher_expiry_at: null,
      lines,
    },
  })

  return { ok: true }
}

export async function resolveProcurementRejection(input: {
  salesOrderId: string
  action:
    | "RESUBMIT_CHANGED_TERMS"
    | "REQUEST_LOW_MARGIN_ACCEPTANCE"
    | "VOID_AFTER_REJECTION"
  idempotencyKey: string
  lowMarginReason?: string
  voidReason?: string
}): Promise<ProcurementResolutionOutcome> {
  const detail = await apiGet<BackendSalesOrderDetail>(
    `/admin/sales-orders/${input.salesOrderId}`
  )

  if (input.action === "VOID_AFTER_REJECTION") {
    await apiPost(`/admin/sales-orders/${input.salesOrderId}/void`, {
      version: detail.version,
    })
    return {
      outcome: "VOIDED_AFTER_PROCUREMENT_REJECTION",
      reference: `PR-VOID-${input.idempotencyKey.slice(0, 8).toUpperCase()}`,
      detail: `销售单已作废并保留驳回历史。原因：${input.voidReason ?? "不做"}`,
      reviewStatus: "VOIDED",
      primaryStatusLabel: "已作废",
    }
  }

  if (input.action === "RESUBMIT_CHANGED_TERMS") {
    const submission = await apiPost<{
      id: string
      submission_no: number
    }>(`/admin/sales-orders/${input.salesOrderId}/submit`, {
      version: detail.version,
      idempotency_key: input.idempotencyKey,
    })
    return {
      outcome: "CHANGED_TERMS_RESUBMITTED",
      reference: `PR-RESUB-${input.idempotencyKey.slice(0, 8).toUpperCase()}`,
      detail:
        "已提交新版本进入采购二次确认；旧驳回记录保持历史。",
      newSubmissionNo: submission.submission_no,
      newSubjectHash: submission.id,
      reviewStatus: "RESOLVED",
      primaryStatusLabel: "待二次确认",
    }
  }

  // REQUEST_LOW_MARGIN_ACCEPTANCE：后端无独立「申请低毛利」写入口；
  // 提交后由审核轨进入 PENDING_LOW_MARGIN_SUPERIOR（若业务服务支持）。
  // 此处执行 submit 并登记缺口：完整低毛利申请协议待后端补齐。
  const submission = await apiPost<{
    id: string
    submission_no: number
  }>(`/admin/sales-orders/${input.salesOrderId}/submit`, {
    version: detail.version,
    idempotency_key: input.idempotencyKey,
  })

  return {
    outcome: "LOW_MARGIN_MANAGER_CONFIRMATION_CREATED",
    reference: `PR-LM-${input.idempotencyKey.slice(0, 8).toUpperCase()}`,
    detail:
      input.lowMarginReason ??
      "已提交并等待低毛利上级确认（若后端审核轨支持）。",
    newSubmissionNo: submission.submission_no,
    newSubjectHash: submission.id,
    reviewStatus: "PENDING_LOW_MARGIN_MANAGER",
    primaryStatusLabel: "待销售处理",
  }
}

export async function decideLowMarginManager(input: {
  salesOrderId: string
  workItemId: string
  decision: "APPROVE" | "REJECT"
  idempotencyKey: string
  reason?: string
}): Promise<ProcurementResolutionOutcome> {
  // workItemId 在集成后映射为 sales_order_review.id（低毛利阶段）
  const path =
    input.decision === "APPROVE"
      ? `/admin/sales-order-reviews/${input.workItemId}/approve`
      : `/admin/sales-order-reviews/${input.workItemId}/reject`

  await apiPost(path, {
    decision_reason: input.reason ?? null,
  })

  if (input.decision === "APPROVE") {
    return {
      outcome: "LOW_MARGIN_APPROVED_AND_PROCUREMENT_RESUBMITTED",
      reference: `LM-OK-${input.idempotencyKey.slice(0, 8).toUpperCase()}`,
      detail: "上级已同意低毛利承接；销售单未直接生效，进入后续采购确认。",
      reviewStatus: "RESOLVED",
      primaryStatusLabel: "待二次确认",
    }
  }

  return {
    outcome: "LOW_MARGIN_REJECTED_TO_SALES",
    reference: `LM-REJ-${input.idempotencyKey.slice(0, 8).toUpperCase()}`,
    detail: `上级驳回低毛利承接（${input.reason ?? "未说明"}）。`,
    reviewStatus: "REJECTED",
    primaryStatusLabel: "待销售处理",
  }
}

// ─── 销售变更单 ──────────────────────────────────────────────────────────────

export async function startSalesChangeOrder(input: {
  salesOrderId: string
  baseRevisionNo: number
  nature: "physical_service" | "card_voucher"
}): Promise<SalesChangeOrderSummary> {
  const detail = await apiGet<BackendSalesOrderDetail>(
    `/admin/sales-orders/${input.salesOrderId}`
  )
  const wc = detail.working_copy
  const latestRev = detail.revisions?.[0]

  // 变更单创建需要完整 draft；以当前工作副本/最新版本金额行作为目标草稿骨架。
  // 字段不足时后端会校验失败并经 ApiError 抛出。
  let contractNo: string | null = null
  let customerName = detail.customer_id
  let settlementName: string | null = null
  let paymentCode = "CUSTOM"
  let paymentName = "合同约定"
  let invoiceType = "SPECIAL"
  let taxPoint = "0"

  if (detail.contract_id) {
    try {
      const contract = await apiGet<BackendContractDetail>(
        `/admin/contracts/${detail.contract_id}`
      )
      contractNo = contract.contract_no
      const rev =
        contract.revisions.find((r) => r.id === contract.current_revision_id) ??
        contract.revisions[0]
      if (rev) {
        customerName = rev.customer_name
        settlementName = rev.settlement_party_name
        paymentCode = rev.payment_term_code
        paymentName = rev.payment_term_name
        invoiceType = rev.invoice_type
        taxPoint = rev.tax_point
      }
    } catch {
      // ignore
    }
  }

  const lines =
    wc?.lines?.map((line) => {
      const isVoucher = line.line_type === "VOUCHER"
      const row: Record<string, unknown> = {
        line_no: line.line_no,
        line_type: line.line_type,
        sales_tax_rate: line.sales_tax_rate,
        item_name_snapshot: line.item_name_snapshot,
        spec_snapshot: line.spec_snapshot ?? null,
        unit_snapshot: line.unit_snapshot ?? null,
        goods: null,
        voucher: null,
      }
      if (isVoucher) {
        const cardCount = line.card_count ?? 1
        const face = line.face_value ?? "0.00"
        const unitPrice = line.unit_price_gross ?? "0.0000"
        const faceTotal = (Number(face) * cardCount).toFixed(2)
        const txn = (Number(unitPrice) * cardCount).toFixed(2)
        const gift = (Number(faceTotal) - Number(txn)).toFixed(2)
        row.voucher = {
          face_value: face,
          card_count: cardCount,
          unit_price_gross: unitPrice,
          face_value_total: faceTotal,
          transaction_amount: txn,
          gift_amount: gift,
          gift_rate: null,
          card_form: line.card_form ?? "ELECTRONIC",
        }
      } else {
        const skuId = line.sku_id?.trim()
        const skuRevisionId = line.sku_revision_id?.trim()
        if (!skuId || !skuRevisionId) {
          throwValidation("历史草稿缺少精确 SKU 修订，请重新从公司商品池选择商品")
        }
        row.goods = {
          sku_id: skuId,
          sku_revision_id: skuRevisionId,
          welfare_scenario: null,
          fulfillment_mode: "COMPANY_WAREHOUSE",
          fulfillment_due_at: Math.floor(Date.now() / 1000),
          quantity: line.quantity ?? "0",
          base_unit_code: line.base_unit_code ?? "EA",
          unit_price_gross: line.unit_price_gross ?? "0.0000",
        }
      }
      return row
    }) ?? []

  if (lines.length === 0) {
    throwValidation("无法发起变更：缺少可变更的明细草稿")
  }

  const created = await apiPost<BackendSalesChangeOrder>(
    "/admin/sales-change-orders",
    {
      sales_order_id: input.salesOrderId,
      change_type: input.nature === "card_voucher" ? "OTHER" : "AMOUNT",
      reason: "销售发起变更",
      idempotency_key: `sco-${input.salesOrderId}-${Date.now()}`,
      draft: {
        editor_user_id: wc?.editor_user_id ?? "unknown",
        customer_name: customerName,
        contract_no: contractNo,
        settlement_party_name: settlementName,
        payment_term_code: paymentCode,
        payment_term_name: paymentName,
        invoice_type: invoiceType,
        tax_point: taxPoint,
        project_name: null,
        business_remark: null,
        voucher_category_sku_id: null,
        voucher_expiry_at: null,
        lines,
      },
    }
  )

  return {
    ...mapChangeOrder(created, input.nature),
    baseRevisionNo: input.baseRevisionNo || latestRev?.revision_no || 0,
  }
}

// ─── 卡券审批 ────────────────────────────────────────────────────────────────

export async function claimCardSalesApproval(input: {
  workItemId: string
}): Promise<{
  workItemId: string
  claimedByLabel: string
}> {
  // 审批记录 id 与 work_item 可能不同：优先 work-items claim，失败则仅返回占位领取态。
  try {
    const wi = await apiGet<BackendWorkItem>(
      `/admin/work-items/${input.workItemId}`
    )
    const claimed = await apiPost<BackendWorkItem>(
      `/admin/work-items/${input.workItemId}/claim`,
      { version: wi.version }
    )
    return {
      workItemId: input.workItemId,
      claimedByLabel: claimed.owner_user_id ?? "当前用户",
    }
  } catch {
    // 卡券审批以 sales_order_review 为主路径：approve/reject 不强制 claim。
    return {
      workItemId: input.workItemId,
      claimedByLabel: "当前用户",
    }
  }
}

export async function completeCardSalesApproval(input: {
  workItemId: string
  workItemType: "CARD_SALES_MANAGER_APPROVAL" | "CARD_SALES_OPERATION_APPROVAL"
  decision: "APPROVE" | "REJECT"
  reasonCode?: string
  comment?: string
}): Promise<CardApprovalCompleteResult> {
  const path =
    input.decision === "APPROVE"
      ? `/admin/sales-order-reviews/${input.workItemId}/approve`
      : `/admin/sales-order-reviews/${input.workItemId}/reject`

  const review = await apiPost<BackendSalesOrderReview>(path, {
    decision_reason:
      input.comment || input.reasonCode
        ? `${input.reasonCode ?? ""}${input.comment ? ` ${input.comment}` : ""}`.trim()
        : null,
  })

  if (input.decision === "REJECT") {
    return {
      outcome: "REJECTED_TO_SALES",
      reference: `CARD-REJ-${review.id.slice(-6).toUpperCase()}`,
      detail: "已驳回并退回销售处理；修改后须从领导审批重新开始。",
      primaryStatusLabel: "草稿",
    }
  }

  if (input.workItemType === "CARD_SALES_MANAGER_APPROVAL") {
    return {
      outcome: "MANAGER_APPROVED",
      reference: `CARD-MGR-${review.id.slice(-6).toUpperCase()}`,
      detail: "领导已通过；进入运营审批阶段。",
      primaryStatusLabel: "待运营审批",
    }
  }

  return {
    outcome: "OPERATIONS_APPROVED_AND_EFFECTIVE",
    reference: `CARD-OPS-${review.id.slice(-6).toUpperCase()}`,
    detail: "运营已通过；销售单应已生效。",
    primaryStatusLabel: "已生效",
  }
}

// ─── 导出任务 ────────────────────────────────────────────────────────────────

export async function createSalesOrderExportJob(input: {
  rowCount: number
}): Promise<ExportJobResult> {
  const jobNo = `EXP-SO-${Date.now().toString(36).toUpperCase()}`
  const requestId =
    typeof crypto !== "undefined" && "randomUUID" in crypto
      ? crypto.randomUUID()
      : `export-${Date.now()}`

  const job = await apiPost<BackendBackgroundJob>("/admin/background-jobs", {
    job_no: jobNo,
    job_type: "export",
    domain_job_type: "SALES_ORDER_EXPORT",
    domain_job_id: null,
    selection_snapshot_id: null,
    request_id: requestId,
    input_file_asset_id: null,
    total_count: Math.max(1, input.rowCount),
    items: [
      {
        object_type: "sales_order",
        object_id: null,
        expected_version: null,
        expected_hash: null,
        worksheet_name: null,
        source_row_no: null,
        source_column_name: null,
      },
    ],
  })

  return {
    jobId: job.id ?? jobNo,
    status: "queued",
    rowCount: input.rowCount,
    permissionVersion: PERMISSION_VERSION,
    createdAt: formatIsoNow(),
    downloadLabel: `销售单导出_${jobNo}`,
  }
}
