/**
 * W06 客户验收 API（queryFn / mutationFn）。
 * 兼容 re-export 旧 detail/acceptance 入口；新 UI 使用 workspace 系列。
 */

import { apiGet, apiPost } from "@/lib/api"
import type { ApiError } from "@/lib/api/errors"
import type {
  AcceptanceEligibleFact,
  AcceptanceHistoryItem,
  AcceptanceOverallResult,
  AcceptanceSalesLineGroup,
  AcceptanceStatus,
  CustomerAcceptanceWorkspaceView,
  FulfillmentFactType,
  PostAcceptanceInput,
  PostAcceptanceResult,
  ReverseAcceptanceInput,
  ReverseAcceptanceResult,
  SaveAcceptanceDraftInput,
} from "@/features/sales-orders/acceptance-types"
import { FACT_ONLY_NOTICE } from "@/features/sales-orders/acceptance-types"
import { fetchSalesOrderDetail } from "@/features/sales-orders/api"



// ─── 后端形状 ────────────────────────────────────────────────────────────────

type BackendEligibleFact = {
  fulfillment_line_id: string
  fulfillment_fact_type: string
  fulfillment_no: string
  sales_order_line_id: string
  line_no: number
  item_snapshot: string
  unit_code?: string | null
  occurred_at: number
  net_successful_quantity: string
  net_accepted_allocated_quantity: string
  eligible_quantity: string
  carrier?: string | null
  tracking_no?: string | null
}

type BackendSalesLineGroup = {
  sales_order_line_id: string
  line_no: number
  item_snapshot: string
  unit_code?: string | null
  required_quantity: string
  net_accepted_quantity: string
  fulfillment_facts: BackendEligibleFact[]
}

type BackendAcceptanceHeader = {
  id: string
  acceptance_no: string
  sales_order_id: string
  accepted_at: number
  result: string
  status: string
  reversal_of_acceptance_id?: string | null
  version: number
  created_at: number
}

type BackendEligibilityView = {
  sales_order_id: string
  sales_lines: BackendSalesLineGroup[]
  history: BackendAcceptanceHeader[]
}

type BackendAcceptanceDetail = {
  acceptance: BackendAcceptanceHeader
  lines: Array<{
    id: string
    line_no: number
    sales_order_line_id: string
    accepted_quantity: string
    short_quantity: string
    rejected_quantity: string
    reason?: string | null
  }>
  allocations: Array<{
    id: string
    customer_acceptance_line_id: string
    fulfillment_fact_type: string
    fulfillment_line_id: string
    allocation_action: string
    allocated_quantity: string
    reverses_allocation_id?: string | null
  }>
}

type PageView<T> = {
  items: T[]
  total: number
  page: number
  page_size: number
}

// ─── 映射 ────────────────────────────────────────────────────────────────────

function formatInstant(secs?: number | null): string {
  if (secs == null || secs <= 0) return ""
  return new Date(secs * 1000).toISOString()
}

function mapFactType(code: string): FulfillmentFactType {
  switch (code) {
    case "ELECTRONIC_DELIVERY":
      return "ELECTRONIC"
    case "SERVICE_FULFILLMENT":
      return "SERVICE"
    case "DELIVERY":
    default:
      // 后端只有 DELIVERY / ELECTRONIC / SERVICE；仓发与代发统一为 WAREHOUSE_SHIP 展示
      return "WAREHOUSE_SHIP"
  }
}

function mapOverallResult(code: string): AcceptanceOverallResult {
  switch (code) {
    case "SHORTAGE":
      return "SHORT"
    case "REJECTED":
      return "REJECT"
    case "SERVICE_FAILED":
      return "SERVICE_FAIL"
    case "PASSED":
    default:
      return "PASS"
  }
}

function mapOverallResultToBackend(
  lines: SaveAcceptanceDraftInput["lines"]
): string {
  if (lines.some((l) => l.serviceFail)) return "SERVICE_FAILED"
  if (lines.some((l) => Number(l.rejectedQuantity) > 0)) return "REJECTED"
  if (lines.some((l) => Number(l.shortQuantity) > 0)) return "SHORTAGE"
  return "PASSED"
}

function mapEligibleFact(f: BackendEligibleFact): AcceptanceEligibleFact {
  return {
    fulfillmentLineId: f.fulfillment_line_id,
    fulfillmentFactType: mapFactType(f.fulfillment_fact_type),
    fulfillmentNo: f.fulfillment_no,
    salesOrderLineId: f.sales_order_line_id,
    lineNo: f.line_no,
    itemSnapshot: f.item_snapshot,
    unitCode: f.unit_code ?? "",
    occurredAt: formatInstant(f.occurred_at),
    netSuccessfulQuantity: f.net_successful_quantity,
    netAcceptedAllocatedQuantity: f.net_accepted_allocated_quantity,
    eligibleQuantity: f.eligible_quantity,
    carrier: f.carrier ?? undefined,
    trackingNo: f.tracking_no ?? undefined,
  }
}

function mapSalesLine(g: BackendSalesLineGroup): AcceptanceSalesLineGroup {
  return {
    salesOrderLineId: g.sales_order_line_id,
    lineNo: g.line_no,
    itemSnapshot: g.item_snapshot,
    unitCode: g.unit_code ?? "",
    requiredQuantity: g.required_quantity,
    netAcceptedQuantity: g.net_accepted_quantity,
    fulfillmentFacts: (g.fulfillment_facts ?? []).map(mapEligibleFact),
  }
}

function mapHistoryItem(
  h: BackendAcceptanceHeader
): AcceptanceHistoryItem | null {
  const status = h.status as AcceptanceStatus
  if (status !== "POSTED" && status !== "REVERSED") return null
  return {
    acceptanceId: h.id,
    acceptanceNo: h.acceptance_no,
    status,
    acceptedAt: formatInstant(h.accepted_at),
    postedAt: formatInstant(h.created_at),
    overallResult: mapOverallResult(h.result),
    lines: [],
    recordedBy: "",
    version: h.version,
    reversalOfAcceptanceId: h.reversal_of_acceptance_id ?? undefined,
    factOnlyNotice: FACT_ONLY_NOTICE,
  }
}

// ─── Workspace API ───────────────────────────────────────────────────────────

export type FetchAcceptanceWorkspaceParams = {
  salesOrderId: string
  remainingOnly?: boolean
  workItemId?: string | null
}

export async function fetchCustomerAcceptanceWorkspace(
  params: FetchAcceptanceWorkspaceParams
): Promise<CustomerAcceptanceWorkspaceView | null> {
  const order = await fetchSalesOrderDetail(params.salesOrderId)
  if (!order) return null

  const workItemConfigBlocker = params.workItemId
    ? "客户验收任务类型尚未注册。请从销售单直接登记验收，不要使用待办队列入口。"
    : null

  const factsUpdatedAt = order.sourceAsOf || new Date().toISOString()

  if (order.nature === "card_voucher") {
    return {
      salesOrder: {
        id: order.id,
        salesOrderNo: order.documentNumber,
        businessType: "CARD_VOUCHER",
        customerLabel: order.customerName,
        commercialStatus: order.primaryStatus.label,
        commercialStatusTone: order.primaryStatus.tone,
        fulfillmentProgress: order.fulfillment.label,
        collectionProgress: order.collection.label,
        invoiceProgress: order.invoicing.label,
        lockVersion: order.lockVersion ?? order.version,
        factsUpdatedAt,
      },
      freshness: { factsUpdatedAt, state: "fresh" },
      metrics: {
        eligibleFulfillmentCount: 0,
        eligibleQuantityByUnit: [],
        overdueLineCount: 0,
      },
      salesLines: [],
      draft: null,
      history: [],
      permissions: {
        allowedActions: [],
        actionBlockers: [
          {
            action: "CREATE_ACCEPTANCE",
            code: "CARD_VOUCHER_NOT_SUPPORTED",
            message:
              "卡券销售单不在客户验收登记；履约完成按销售单履约期限判断。",
          },
        ],
        fieldVisibility: { customerName: "full" },
      },
      workItem: null,
      lease: null,
      workItemConfigBlocker,
    }
  }

  let eligibility: BackendEligibilityView
  try {
    eligibility = await apiGet<BackendEligibilityView>(
      "/admin/customer-acceptances/eligible",
      { sales_order_id: params.salesOrderId }
    )
  } catch (err) {
    const apiErr = err as ApiError
    if (apiErr?.status === 404) return null
    throw err
  }

  const remainingOnly = params.remainingOnly !== false
  let salesLines = (eligibility.sales_lines ?? []).map(mapSalesLine)
  if (remainingOnly) {
    salesLines = salesLines
      .map((line) => ({
        ...line,
        fulfillmentFacts: line.fulfillmentFacts.filter(
          (f) => Number(f.eligibleQuantity) > 0
        ),
      }))
      .filter(
        (line) =>
          line.fulfillmentFacts.length > 0 ||
          Number(line.netAcceptedQuantity) > 0
      )
  }

  const allFacts = salesLines.flatMap((l) => l.fulfillmentFacts)
  const eligibleFacts = allFacts.filter((f) => Number(f.eligibleQuantity) > 0)
  const qtyByUnit = new Map<string, number>()
  for (const fact of eligibleFacts) {
    qtyByUnit.set(
      fact.unitCode,
      (qtyByUnit.get(fact.unitCode) ?? 0) + Number(fact.eligibleQuantity)
    )
  }

  const history = (eligibility.history ?? [])
    .map(mapHistoryItem)
    .filter((h): h is AcceptanceHistoryItem => h != null)

  // 草稿：取最新 DRAFT 验收单（若有）
  let draft: CustomerAcceptanceWorkspaceView["draft"] = null
  try {
    const draftPage = await apiGet<PageView<BackendAcceptanceHeader>>(
      "/admin/customer-acceptances",
      {
        sales_order_id: params.salesOrderId,
        status: "DRAFT",
        page: 1,
        page_size: 1,
        sort_by: "created_at",
        sort_dir: "desc",
      }
    )
    const header = draftPage.items[0]
    if (header) {
      const detail = await apiGet<BackendAcceptanceDetail>(
        `/admin/customer-acceptances/${header.id}`
      )
      draft = {
        acceptanceDraftId: header.id,
        draftVersion: header.version,
        salesOrderId: params.salesOrderId,
        acceptedAt: formatInstant(header.accepted_at),
        comment: "",
        lines: detail.lines.map((line) => ({
          salesOrderLineId: line.sales_order_line_id,
          acceptedQuantity: line.accepted_quantity,
          shortQuantity: line.short_quantity,
          rejectedQuantity: line.rejected_quantity,
          reason: line.reason ?? "",
          allocations: detail.allocations
            .filter((a) => a.customer_acceptance_line_id === line.id)
            .map((a) => ({
              fulfillmentLineId: a.fulfillment_line_id,
              allocatedQuantity: a.allocated_quantity,
            })),
        })),
        updatedAt: formatInstant(header.created_at),
      }
    }
  } catch {
    // 草稿读取失败不阻塞工作台
  }

  const allowedActions = ["CREATE_ACCEPTANCE", "POST_ACCEPTANCE", "SAVE_DRAFT"]
  if (history.some((h) => h.status === "POSTED")) {
    allowedActions.push("REVERSE_ACCEPTANCE")
  }

  return {
    salesOrder: {
      id: order.id,
      salesOrderNo: order.documentNumber,
      businessType: "GOODS_SERVICE",
      customerLabel: order.customerName,
      commercialStatus: order.primaryStatus.label,
      commercialStatusTone: order.primaryStatus.tone,
      fulfillmentProgress: order.fulfillment.label,
      collectionProgress: order.collection.label,
      invoiceProgress: order.invoicing.label,
      lockVersion: order.lockVersion ?? order.version,
      factsUpdatedAt,
    },
    freshness: { factsUpdatedAt, state: "fresh" },
    metrics: {
      eligibleFulfillmentCount: eligibleFacts.length,
      eligibleQuantityByUnit: [...qtyByUnit.entries()].map(
        ([unitCode, quantity]) => ({
          unitCode,
          quantity: String(quantity),
        })
      ),
      overdueLineCount: 0,
    },
    salesLines,
    draft,
    history,
    permissions: {
      allowedActions,
      actionBlockers: [],
      fieldVisibility: {
        customerName: "full",
        customerContact: "full",
      },
    },
    workItem: null,
    lease: null,
    workItemConfigBlocker,
  }
}

export async function saveCustomerAcceptanceDraft(
  input: SaveAcceptanceDraftInput
) {
  // 后端无独立「保存草稿」接口：POST 创建 DRAFT 验收单。
  // 已有 draft id 时无法局部更新（缺口），重新创建一笔草稿。
  const acceptanceNo =
    input.acceptanceDraftId && input.acceptanceDraftId.startsWith("YS")
      ? input.acceptanceDraftId
      : `YS${Date.now().toString(36).toUpperCase()}`

  const acceptedAtSecs = input.acceptedAt
    ? Math.floor(Date.parse(input.acceptedAt) / 1000) ||
      Math.floor(Date.now() / 1000)
    : Math.floor(Date.now() / 1000)

  const created = await apiPost<BackendAcceptanceDetail | BackendAcceptanceHeader>(
    "/admin/customer-acceptances",
    {
      acceptance_no: acceptanceNo,
      sales_order_id: input.salesOrderId,
      accepted_at: acceptedAtSecs,
      result: mapOverallResultToBackend(input.lines),
      lines: input.lines.map((line) => ({
        sales_order_line_id: line.salesOrderLineId,
        accepted_quantity: line.acceptedQuantity || "0",
        short_quantity: line.shortQuantity || "0",
        rejected_quantity: line.rejectedQuantity || "0",
        reason: line.reason || null,
        allocations: line.allocations.map((a) => ({
          fulfillment_line_id: a.fulfillmentLineId,
          fulfillment_fact_type: "DELIVERY",
          allocated_quantity: a.allocatedQuantity || "0",
        })),
      })),
    }
  )

  const header =
    "acceptance" in created && created.acceptance
      ? created.acceptance
      : (created as BackendAcceptanceHeader)

  return {
    acceptanceDraftId: header.id,
    draftVersion: header.version,
    salesOrderId: input.salesOrderId,
    acceptedAt: input.acceptedAt,
    comment: input.comment,
    lines: input.lines,
    updatedAt: new Date().toISOString(),
  }
}

export async function postCustomerAcceptanceWorkspace(
  input: PostAcceptanceInput
): Promise<PostAcceptanceResult> {
  try {
    let acceptanceId = input.acceptanceDraftId

    // 若无服务端草稿 id，先创建
    if (!acceptanceId || acceptanceId.startsWith("draft_")) {
      const saved = await saveCustomerAcceptanceDraft({
        salesOrderId: input.salesOrderId,
        acceptedAt: input.acceptedAt,
        comment: input.comment,
        lines: input.lines,
      })
      acceptanceId = saved.acceptanceDraftId
    }

    // 解析履约事实类型：优先用工作台已加载的类型映射
    const eligibility = await apiGet<BackendEligibilityView>(
      "/admin/customer-acceptances/eligible",
      { sales_order_id: input.salesOrderId }
    ).catch(() => null)

    const factTypeByLineId = new Map<string, string>()
    for (const group of eligibility?.sales_lines ?? []) {
      for (const fact of group.fulfillment_facts ?? []) {
        factTypeByLineId.set(fact.fulfillment_line_id, fact.fulfillment_fact_type)
      }
    }

    const posted = await apiPost<BackendAcceptanceDetail>(
      `/admin/customer-acceptances/${acceptanceId}/post`,
      {
        lines: input.lines.map((line) => ({
          sales_order_line_id: line.salesOrderLineId,
          allocations: line.allocations.map((a) => ({
            fulfillment_line_id: a.fulfillmentLineId,
            fulfillment_fact_type:
              factTypeByLineId.get(a.fulfillmentLineId) ?? "DELIVERY",
            allocated_quantity: a.allocatedQuantity || "0",
          })),
        })),
      }
    )

    const header = posted.acceptance ?? (posted as unknown as BackendAcceptanceHeader)
    const overall = mapOverallResult(header.result)

    // 估算剩余可验收
    let remainingEligibleCount = 0
    if (eligibility) {
      remainingEligibleCount = eligibility.sales_lines
        .flatMap((g) => g.fulfillment_facts)
        .filter((f) => Number(f.eligible_quantity) > 0).length
    }

    return {
      status: "succeeded",
      acceptanceNo: header.acceptance_no,
      acceptanceId: header.id,
      remainingEligibleCount,
      remainingEligibleQuantityLabel: "",
      overallResult: overall,
      factOnlyNotice: FACT_ONLY_NOTICE,
    }
  } catch (err) {
    const apiErr = err as ApiError
    if (apiErr?.kind === "Network" || apiErr?.status === 500) {
      return {
        status: "unknown",
        message:
          apiErr.message ||
          "操作结果暂无法确认，请查询当前状态后再决定是否重试",
        idempotencyKey: input.idempotencyKey,
      }
    }
    return {
      status: "failed",
      message: apiErr?.message ?? "验收过账失败",
    }
  }
}

export async function reverseCustomerAcceptanceWorkspace(
  input: ReverseAcceptanceInput
): Promise<ReverseAcceptanceResult> {
  try {
    const reversed = await apiPost<BackendAcceptanceDetail>(
      `/admin/customer-acceptances/${input.acceptanceId}/reverse`,
      {
        expected_version: input.expectedAcceptanceVersion,
        reason_text: input.reasonText,
      }
    )
    const header =
      reversed.acceptance ?? (reversed as unknown as BackendAcceptanceHeader)
    return {
      status: "succeeded",
      reverseAcceptanceNo: header.acceptance_no,
      reverseAcceptanceId: header.id,
      originalAcceptanceNo: input.acceptanceId,
    }
  } catch (err) {
    const apiErr = err as ApiError
    return {
      status: "failed",
      message: apiErr?.message ?? "冲正失败",
    }
  }
}
