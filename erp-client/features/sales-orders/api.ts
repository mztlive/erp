import { mockDelay } from "@/lib/mock-delay"
import type {
  CardSalesApproval,
  CreateSalesOrderInput,
  CreateSalesOrderResult,
  ProcurementRejectionResolution,
  SalesOrderLineItem,
  SalesOrderListItem,
} from "@/features/sales-orders/types"
import {
  computeSalesOrderMetrics,
  filterSalesOrders,
  type SalesOrderNatureFilter,
  type SalesOrderOriginFilter,
  type SalesOrderStatusFilter,
  type SalesOrderSummaryFilter,
} from "@/features/sales-orders/filter-orders"
import { buildSalesOrder } from "@/features/sales-orders/build-order"
import {
  getMockSalesOrder,
  listMockSalesOrders,
  registerMockSalesOrder,
} from "@/mock/sales-orders"
import {
  claimW05CardApproval,
  completeW05CardApproval,
  createW05ExportJob,
  decideW05LowMargin,
  getSalesOrderAcceptance,
  getW05CardLeasePublic,
  getW05CardTerminal,
  getW05ChangeOrder,
  getW05DraftPriceAdjusted,
  getW05RejectionOutcome,
  getW04ContractCenter,
  hasW05DraftPriceAdjusted,
  linkW04SalesOrder,
  markW05DraftPriceAdjusted,
  postSalesOrderAcceptance,
  resolveW05ProcurementRejection,
  startW05SalesChangeOrder,
} from "@/mock/session-state"
import {
  canonicalDecimal,
  compareDecimal,
  multiplyFixed,
  splitGrossByPercentRate,
  sumFixed,
} from "@/lib/fixed-decimal"

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
  nature?: SalesOrderNatureFilter
  summary?: SalesOrderSummaryFilter
  origin?: SalesOrderOriginFilter
  status?: SalesOrderStatusFilter
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

const PERMISSION_VERSION = "pv-w05-demo-1"

function mergeSessionOverlay(order: SalesOrderListItem): SalesOrderListItem {
  const rejectionOutcome = getW05RejectionOutcome(order.id)
  const changeOrder = getW05ChangeOrder(order.id)
  const draftAdjust = getW05DraftPriceAdjusted(order.id)

  let next: SalesOrderListItem = { ...order }

  if (changeOrder) {
    next = {
      ...next,
      activeChangeOrder: changeOrder,
      allowedActions: next.allowedActions.filter(
        (a) => a !== "START_SALES_CHANGE"
      ),
      actionBlockers: [
        ...next.actionBlockers.filter((b) => b.action !== "START_SALES_CHANGE"),
        {
          action: "START_SALES_CHANGE",
          reason: "已有进行中的销售变更单。",
        },
      ],
    }
  }

  if (order.procurementRejection) {
    let procurementRejection: ProcurementRejectionResolution = {
      ...order.procurementRejection,
    }

    if (draftAdjust && procurementRejection.reviewStatus === "REJECTED") {
      procurementRejection = {
        ...procurementRejection,
        draftDifference: {
          changedItemOrService: false,
          changedSalesPrice: true,
          commercialTermsUnchanged: false,
          diffSummary: [
            {
              field: "销售含税单价（主明细）",
              before: order.lineItems[0]?.unitPriceGross ?? "—",
              after: draftAdjust.unitPriceGross,
            },
            {
              field: "调整说明",
              before: "—",
              after: draftAdjust.note,
            },
          ],
        },
        allowedActions: [
          "RESUBMIT_CHANGED_TERMS",
          "REQUEST_LOW_MARGIN_ACCEPTANCE",
          "VOID_AFTER_REJECTION",
        ],
        actionBlockers: procurementRejection.actionBlockers.filter(
          (b) => b.action !== "RESUBMIT_CHANGED_TERMS"
        ),
      }
    }

    if (rejectionOutcome) {
      procurementRejection = {
        ...procurementRejection,
        reviewStatus:
          rejectionOutcome.reviewStatus ?? procurementRejection.reviewStatus,
        resolutionOutcome: {
          outcome: rejectionOutcome.outcome,
          reference: rejectionOutcome.reference,
          detail: rejectionOutcome.detail,
          newSubmissionNo: rejectionOutcome.newSubmissionNo,
          newSubjectHash: rejectionOutcome.newSubjectHash,
          newWorkItemId: rejectionOutcome.newWorkItemId,
        },
      }

      if (rejectionOutcome.outcome === "LOW_MARGIN_MANAGER_CONFIRMATION_CREATED") {
        procurementRejection = {
          ...procurementRejection,
          reviewStatus: "PENDING_LOW_MARGIN_MANAGER",
          lowMarginSubmission: {
            submissionId: `sub_lm_${order.id}`,
            submissionNo: rejectionOutcome.newSubmissionNo ?? 2,
            subjectHash:
              rejectionOutcome.newSubjectHash ?? "sha256:lm…pending",
            acceptanceReason: "照原条件申请低毛利承接（演示）",
            commercialTermsMatchRejectedSubmission: true,
          },
          activeLowMarginManagerTask: {
            workItemId:
              rejectionOutcome.newWorkItemId ?? `wi_lm_${order.id}`,
            workItemType: "LOW_MARGIN_MANAGER_CONFIRMATION",
            workItemStatus: "UNCLAIMED",
            subjectHash:
              rejectionOutcome.newSubjectHash ?? "sha256:lm…pending",
            allowedActions: ["CLAIM", "APPROVE", "REJECT"],
            actionBlockers: [],
          },
          allowedActions: [],
          actionBlockers: [
            {
              action: "RESUBMIT_CHANGED_TERMS",
              reason: "低毛利上级确认进行中，不可并行选择其它出路。",
            },
            {
              action: "REQUEST_LOW_MARGIN_ACCEPTANCE",
              reason: "已有有效低毛利任务。",
            },
            {
              action: "VOID_AFTER_REJECTION",
              reason: "存在有效的低毛利后继任务，不可同时作废。",
            },
          ],
        }
      }

      if (
        rejectionOutcome.outcome === "CHANGED_TERMS_RESUBMITTED" ||
        rejectionOutcome.outcome ===
          "LOW_MARGIN_APPROVED_AND_PROCUREMENT_RESUBMITTED"
      ) {
        next = {
          ...next,
          primaryStatus: { label: "待二次确认", tone: "warning" },
          procurementRejection: {
            ...procurementRejection,
            reviewStatus: "RESOLVED",
            allowedActions: [],
            actionBlockers: [],
            activeLowMarginManagerTask: undefined,
          },
        }
        return next
      }

      if (rejectionOutcome.outcome === "VOIDED_AFTER_PROCUREMENT_REJECTION") {
        next = {
          ...next,
          primaryStatus: { label: "已作废", tone: "void" },
          procurementRejection: {
            ...procurementRejection,
            reviewStatus: "VOIDED",
            allowedActions: [],
            actionBlockers: [],
          },
        }
        return next
      }

      if (rejectionOutcome.outcome === "LOW_MARGIN_REJECTED_TO_SALES") {
        procurementRejection = {
          ...procurementRejection,
          reviewStatus: "REJECTED",
          activeLowMarginManagerTask: undefined,
          lowMarginSubmission: undefined,
          allowedActions: [
            "RESUBMIT_CHANGED_TERMS",
            "REQUEST_LOW_MARGIN_ACCEPTANCE",
            "VOID_AFTER_REJECTION",
          ],
          actionBlockers: hasW05DraftPriceAdjusted(order.id)
            ? []
            : [
                {
                  action: "RESUBMIT_CHANGED_TERMS",
                  reason:
                    "还没改商品或价格，请先保存改价后再报给采购。",
                },
              ],
        }
      }

      next = { ...next, procurementRejection }
    } else {
      next = { ...next, procurementRejection }
    }
  }

  if (order.activeCardSalesApproval) {
    const terminal = getW05CardTerminal(order.activeCardSalesApproval.workItemId)
    const lease = getW05CardLeasePublic(order.activeCardSalesApproval.workItemId)

    if (terminal) {
      if (terminal.outcome === "MANAGER_APPROVED") {
        const opsApproval: CardSalesApproval = {
          workItemId: terminal.nextWorkItemId ?? "wi_card_ops_next",
          workItemType: "CARD_SALES_OPERATION_APPROVAL",
          workItemStatus: "UNCLAIMED",
          subjectVersion: order.activeCardSalesApproval.subjectVersion,
          subjectHash: order.activeCardSalesApproval.subjectHash,
          frozenSubmissionSummary:
            order.activeCardSalesApproval.frozenSubmissionSummary,
          expectedReviewStatus: "PENDING_OPERATIONS",
          allowedActions: ["CLAIM"],
          actionBlockers: [
            {
              action: "APPROVE",
              reason: "请先领取后再审批。",
            },
            {
              action: "REJECT",
              reason: "请先领取后再审批。",
            },
          ],
        }
        next = {
          ...next,
          primaryStatus: { label: "待运营审批", tone: "warning" },
          activeCardSalesApproval: opsApproval,
        }
      } else if (terminal.outcome === "OPERATIONS_APPROVED_AND_EFFECTIVE") {
        next = {
          ...next,
          primaryStatus: { label: "已生效", tone: "success" },
          activeCardSalesApproval: null,
          commercialReadOnly: true,
          commercialReadOnlyReason:
            "本单已生效，不能直接改；改内容请「发起改单」，并完成后续确认与财务复核。",
        }
      } else {
        next = {
          ...next,
          primaryStatus: { label: "草稿", tone: "neutral" },
          activeCardSalesApproval: null,
        }
      }
    } else if (lease) {
      next = {
        ...next,
        activeCardSalesApproval: {
          ...order.activeCardSalesApproval,
          workItemStatus: "CLAIMED",
          claimedByLabel: lease.claimedByLabel,
          allowedActions: ["APPROVE", "REJECT"],
          actionBlockers: [],
        },
      }
    }
  }

  return next
}

function sortSalesOrders(
  orders: readonly SalesOrderListItem[],
  sortBy: SalesOrdersListQuery["sortBy"],
  sortDir: SalesOrdersListQuery["sortDir"]
): SalesOrderListItem[] {
  if (!sortBy) return [...orders]
  const direction = sortDir === "asc" ? 1 : -1
  return [...orders].sort((a, b) => {
    const comparison =
      sortBy === "amountGross"
        ? compareDecimal(a.amountGross, b.amountGross, 6)
        : a[sortBy].localeCompare(b[sortBy])
    if (comparison !== 0) return comparison * direction
    return a.submittedAt.localeCompare(b.submittedAt)
  })
}

export async function fetchSalesOrders(
  query: SalesOrdersListQuery
): Promise<SalesOrderListView> {
  await mockDelay()
  const all = listMockSalesOrders().map(mergeSessionOverlay)
  const metrics = computeSalesOrderMetrics(all)
  const filtered = filterSalesOrders(all, {
    search: query.search,
    natureFilter: query.nature,
    summaryFilter: query.summary,
    originFilter: query.origin,
    statusFilter: query.status,
  })
  const sorted = sortSalesOrders(filtered, query.sortBy, query.sortDir)
  const page = Math.max(1, query.page)
  const pageSize = Math.max(1, query.pageSize)
  const start = (page - 1) * pageSize
  return {
    items: sorted.slice(start, start + pageSize),
    total: sorted.length,
    page,
    pageSize,
    metrics,
    queriedAt: new Date().toISOString(),
  }
}

export async function fetchSalesOrderDetail(
  id: string
): Promise<SalesOrderDetailView | null> {
  await mockDelay()
  const base = getMockSalesOrder(id)
  if (!base) return null
  const order = mergeSessionOverlay(base)
  const queriedAt = new Date().toISOString()
  return {
    ...order,
    acceptance: getSalesOrderAcceptance(id),
    permissionVersion: PERMISSION_VERSION,
    sourceAsOf: queriedAt,
    queriedAt,
  }
}

function localDateParts(date: Date) {
  const parts = new Intl.DateTimeFormat("zh-CN", {
    timeZone: "Asia/Shanghai",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
  }).formatToParts(date)
  return Object.fromEntries(parts.map((part) => [part.type, part.value]))
}

/**
 * M5 建单 mock：选择已有合同，或在同一幂等操作中上传 PDF、归档合同并建单。
 * 保存草稿只做宽松校验（至少一行明细）；提交才要求明细完整可计算。
 */
export async function createSalesOrder(
  input: CreateSalesOrderInput
): Promise<CreateSalesOrderResult> {
  await mockDelay(180)

  if (input.lineItems.length === 0) throw new Error("LINE_ITEM_REQUIRED")
  if (input.nature === "card_voucher" && input.lineItems.length !== 1) {
    throw new Error("VOUCHER_REQUIRES_EXACTLY_ONE_LINE")
  }

  const isDraft = input.intent === "SAVE_DRAFT"

  const safeDecimal = (value: string, maxScale: number): string => {
    try {
      return canonicalDecimal(value, { maxScale })
    } catch {
      return "0"
    }
  }

  if (!isDraft) {
    let decimalInputsValid = true
    try {
      for (const line of input.lineItems) {
        decimalInputsValid &&= compareDecimal(line.quantity, "0", 6) > 0
        decimalInputsValid &&= compareDecimal(line.unitPriceGross, "0", 4) > 0
        if (input.nature === "card_voucher") {
          decimalInputsValid &&= compareDecimal(line.faceValue, "0", 2) > 0
          canonicalDecimal(line.giftRate || "0", { maxScale: 6 })
        }
      }
      const taxRate = canonicalDecimal(input.taxRatePercent, { maxScale: 6 })
      decimalInputsValid &&= compareDecimal(taxRate, "0", 6) >= 0
      decimalInputsValid &&= compareDecimal(taxRate, "100", 6) <= 0
    } catch {
      decimalInputsValid = false
    }

    if (
      !decimalInputsValid ||
      input.lineItems.some(
        (line) =>
          !line.name.trim() ||
          !line.unit.trim() ||
          (input.nature === "card_voucher" && !line.cardForm.trim()) ||
          (input.nature === "physical_service" &&
            (!line.fulfillmentMode.trim() || !line.dueDate))
      )
    ) {
      throw new Error("LINE_ITEM_INVALID")
    }
  }

  const contractId = input.contract.contractId
  const requestedRevisionId = input.contract.requestedContractRevisionId

  const taxRate = safeDecimal(input.taxRatePercent, 6)

  const contract = getW04ContractCenter(contractId)
  if (!contract?.selectableForNewSalesOrder) {
    throw new Error(contract?.selectableBlocker ?? "CONTRACT_NOT_SELECTABLE")
  }

  const requestedRevision = requestedRevisionId
    ? contract.revisionTimeline.find(
        (revision) => revision.revisionId === requestedRevisionId
      )
    : contract.revisionTimeline.find((revision) => revision.isCurrent)
  if (!requestedRevision) throw new Error("CONTRACT_REVISION_NOT_FOUND")
  if (!requestedRevision.isCurrent) throw new Error("CONTRACT_REVISION_NOT_CURRENT")

  const computedLines = input.lineItems.map((line, index) => {
    const quantity = safeDecimal(line.quantity, 6)
    const unitPriceGross = safeDecimal(line.unitPriceGross, 4)
    const amountGross = multiplyFixed(quantity, unitPriceGross, {
      leftMaxScale: 6,
      rightMaxScale: 4,
      outputScale: 2,
    })
    const amounts = splitGrossByPercentRate(amountGross, taxRate)
    const item: SalesOrderLineItem = {
      id: `li_${index + 1}`,
      name: line.name.trim(),
      sku: line.sku.trim() || undefined,
      quantity,
      unit: line.unit.trim(),
      unitPriceGross,
      amountGross,
      ...(input.nature === "card_voucher"
        ? {
            faceValue: safeDecimal(line.faceValue, 2),
            giftRate: safeDecimal(line.giftRate || "0", 6),
            cardForm: line.cardForm.trim(),
          }
        : {
            fulfillmentMode: line.fulfillmentMode.trim(),
            dueDate: line.dueDate,
          }),
    }
    return { item, amounts }
  })
  const lineItems = computedLines.map(({ item }) => item)
  const gross = sumFixed(
    computedLines.map(({ amounts }) => amounts.gross),
    { maxScale: 2, outputScale: 2 }
  )
  const net = sumFixed(
    computedLines.map(({ amounts }) => amounts.net),
    { maxScale: 2, outputScale: 2 }
  )
  const tax = sumFixed(
    computedLines.map(({ amounts }) => amounts.tax),
    { maxScale: 2, outputScale: 2 }
  )
  const now = new Date()
  const date = localDateParts(now)
  const sequence = listMockSalesOrders().length + 1
  const documentNumber = `XS${date.year}${date.month}${date.day}${String(sequence).padStart(3, "0")}`
  const salesOrderId = `so_${now.getTime().toString(36)}_${sequence}`
  const submittedAt = `${date.year}-${date.month}-${date.day} ${date.hour}:${date.minute}`
  const submitted = input.intent === "SUBMIT"

  const activeCardSalesApproval: CardSalesApproval | null =
    submitted && input.nature === "card_voucher"
      ? {
          workItemId: `wi_card_mgr_${salesOrderId}`,
          workItemType: "CARD_SALES_MANAGER_APPROVAL",
          workItemStatus: "UNCLAIMED",
          subjectVersion: "sub:1",
          subjectHash: `sha256:${salesOrderId.slice(-10)}…draft`,
          frozenSubmissionSummary: `${lineItems[0]?.name ?? "卡券"} · ${lineItems[0]?.quantity ?? "0"} 张 · 面值 ${lineItems[0]?.faceValue ?? "0.00"} · ${lineItems[0]?.cardForm ?? "—"} · 履约期限至 ${input.fulfillmentDeadline} · 含税 ${gross}`,
          expectedReviewStatus: "PENDING_SALES_LEAD",
          allowedActions: ["CLAIM"],
          actionBlockers: [
            { action: "APPROVE", reason: "请先领取后再审批。" },
            { action: "REJECT", reason: "请先领取后再审批。" },
          ],
        }
      : null

  const created = registerMockSalesOrder(
    buildSalesOrder({
      id: salesOrderId,
      documentNumber,
      customerName: contract.customer.displayName,
      contractNumber: contract.contractNo,
      contractRevisionLabel: `${contract.contractNo}@v${requestedRevision.revisionNo}`,
      nature: input.nature,
      originSystem: "erp",
      primaryStatus: submitted
        ? input.nature === "card_voucher"
          ? { label: "待销售领导审批", tone: "warning" }
          : { label: "待二次确认", tone: "warning" }
        : { label: "草稿", tone: "neutral" },
      fulfillment: { label: "未开始", tone: "neutral" },
      collection: { label: "未收", tone: "neutral" },
      invoicing: { label: "未开", tone: "neutral" },
      amountGross: gross,
      amountNet: net,
      taxAmount: tax,
      receivedAmount: "0.00",
      invoicedAmount: "0.00",
      ownerName: input.ownerName.trim(),
      submittedAt,
      welfareScene: input.welfareScene.trim(),
      remark: input.remark.trim() || undefined,
      version: 1,
      settlementEntity: contract.currentRevision.settlementParty.displayName,
      sellerEntity: "某某福利科技有限公司",
      paymentTerms: input.paymentTerms.trim(),
      fulfillmentDeadline: input.fulfillmentDeadline,
      lineItems,
      related: {
        purchaseOrders: 0,
        fulfillments: 0,
        receipts: 0,
        invoices: 0,
      },
      activeCardSalesApproval,
    }),
    input.idempotencyKey
  )

  linkW04SalesOrder({
    contractId: contract.contractId,
    salesOrderId: created.id,
    documentNumber: created.documentNumber,
    natureLabel: input.nature === "card_voucher" ? "卡券" : "实物与服务",
    contractRevisionNo: requestedRevision.revisionNo,
    statusLabel: created.primaryStatus.label,
    statusTone: created.primaryStatus.tone,
    amountGross: created.amountGross,
  })

  return {
    salesOrderId: created.id,
    documentNumber: created.documentNumber,
    statusLabel: created.primaryStatus.label,
    createdAt: now.toISOString(),
    reference: `SO-CREATE-${created.documentNumber}`,
  }
}

export async function submitSalesOrderAcceptance(input: {
  salesOrderId: string
  documentNumber: string
  acceptedQuantity: string
  note: string
}): Promise<{ reference: string }> {
  await mockDelay(150)
  const reference = `AC-${input.documentNumber}-${Date.now().toString(36).toUpperCase()}`
  postSalesOrderAcceptance(input.salesOrderId, {
    acceptedQuantity: input.acceptedQuantity,
    note: input.note,
    reference,
  })
  return { reference }
}

export async function adjustProcurementRejectionDraft(input: {
  salesOrderId: string
  unitPriceGross: string
  note: string
}): Promise<{ ok: true }> {
  await mockDelay(120)
  markW05DraftPriceAdjusted(
    input.salesOrderId,
    input.unitPriceGross,
    input.note
  )
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
}): Promise<ReturnType<typeof resolveW05ProcurementRejection>> {
  await mockDelay(180)
  const cached = getW05RejectionOutcome(input.salesOrderId)
  if (cached) return cached
  return resolveW05ProcurementRejection({
    salesOrderId: input.salesOrderId,
    action: input.action,
    lowMarginReason: input.lowMarginReason,
    voidReason: input.voidReason,
    priceAdjusted: hasW05DraftPriceAdjusted(input.salesOrderId),
  })
}

export async function decideLowMarginManager(input: {
  salesOrderId: string
  workItemId: string
  decision: "APPROVE" | "REJECT"
  idempotencyKey: string
  reason?: string
}): Promise<ReturnType<typeof decideW05LowMargin>> {
  await mockDelay(180)
  return decideW05LowMargin({
    salesOrderId: input.salesOrderId,
    workItemId: input.workItemId,
    decision: input.decision,
    reason: input.reason,
  })
}

export async function startSalesChangeOrder(input: {
  salesOrderId: string
  baseRevisionNo: number
  nature: "physical_service" | "card_voucher"
}): Promise<ReturnType<typeof startW05SalesChangeOrder>> {
  await mockDelay(150)
  return startW05SalesChangeOrder(input)
}

export async function claimCardSalesApproval(input: {
  workItemId: string
}): Promise<{
  workItemId: string
  claimedByLabel: string
}> {
  await mockDelay(100)
  const lease = claimW05CardApproval(input.workItemId)
  return {
    workItemId: input.workItemId,
    claimedByLabel: lease.claimedByLabel,
  }
}

export async function completeCardSalesApproval(input: {
  workItemId: string
  workItemType: "CARD_SALES_MANAGER_APPROVAL" | "CARD_SALES_OPERATION_APPROVAL"
  decision: "APPROVE" | "REJECT"
  reasonCode?: string
  /** 驳回说明：随驳回送达销售（演示 mock 未持久化，接口已透传）。 */
  comment?: string
}): Promise<ReturnType<typeof completeW05CardApproval>> {
  await mockDelay(180)
  return completeW05CardApproval(input)
}

export async function createSalesOrderExportJob(input: {
  rowCount: number
}): Promise<ReturnType<typeof createW05ExportJob>> {
  await mockDelay(200)
  return createW05ExportJob({
    rowCount: input.rowCount,
    permissionVersion: PERMISSION_VERSION,
  })
}

export { PERMISSION_VERSION }
