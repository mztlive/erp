import { mockDelay } from "@/features/workspace-kit/delay"
import type {
  CardSalesApproval,
  CreateSalesOrderInput,
  CreateSalesOrderResult,
  ProcurementRejectionResolution,
  SalesOrderLineItem,
  SalesOrderListItem,
} from "@/features/sales-orders/types"
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
  getW05RejectionByIdempotency,
  getW05RejectionOutcome,
  getW04ContractCenter,
  hasW05DraftPriceAdjusted,
  linkW04SalesOrder,
  markW05DraftPriceAdjusted,
  postSalesOrderAcceptance,
  resolveW05ProcurementRejection,
  startW05SalesChangeOrder,
  verifyW05CardClaim,
} from "@/mock/session-state"
import {
  canonicalDecimal,
  compareDecimal,
  multiplyFixed,
  normalizeFixed,
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

export type SalesOrderListView = {
  rows: SalesOrderListItem[]
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
          leaseExpiresAt: lease.expiresAt,
          allowedActions: ["APPROVE", "REJECT"],
          actionBlockers: [],
        },
      }
    }
  }

  return next
}

export async function fetchSalesOrders(): Promise<SalesOrderListView> {
  await mockDelay()
  return {
    rows: listMockSalesOrders().map(mergeSessionOverlay),
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
 */
export async function createSalesOrder(
  input: CreateSalesOrderInput
): Promise<CreateSalesOrderResult> {
  await mockDelay(180)

  if (input.lineItems.length === 0) throw new Error("LINE_ITEM_REQUIRED")
  if (input.nature === "card_voucher" && input.lineItems.length !== 1) {
    throw new Error("VOUCHER_REQUIRES_EXACTLY_ONE_LINE")
  }
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

  const contractId = input.contract.contractId
  const requestedRevisionId = input.contract.requestedContractRevisionId

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
    const quantity = canonicalDecimal(line.quantity, { maxScale: 6 })
    const unitPriceGross = canonicalDecimal(line.unitPriceGross, { maxScale: 4 })
    const amountGross = multiplyFixed(quantity, unitPriceGross, {
      leftMaxScale: 6,
      rightMaxScale: 4,
      outputScale: 2,
    })
    const amounts = splitGrossByPercentRate(amountGross, input.taxRatePercent)
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
            faceValue: normalizeFixed(line.faceValue, {
              maxScale: 2,
              outputScale: 2,
            }),
            giftRate: canonicalDecimal(line.giftRate || "0", { maxScale: 6 }),
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
  const cached = getW05RejectionByIdempotency(input.idempotencyKey)
  if (cached) return cached
  return resolveW05ProcurementRejection({
    ...input,
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
  return decideW05LowMargin(input)
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
  claimToken: string
  leaseVersion: number
  claimedByLabel: string
  expiresAt: string
}> {
  await mockDelay(100)
  return claimW05CardApproval(input.workItemId)
}

export async function completeCardSalesApproval(input: {
  workItemId: string
  workItemType: "CARD_SALES_MANAGER_APPROVAL" | "CARD_SALES_OPERATION_APPROVAL"
  decision: "APPROVE" | "REJECT"
  claimToken: string
  leaseVersion: number
  idempotencyKey: string
  reasonCode?: string
}): Promise<ReturnType<typeof completeW05CardApproval>> {
  await mockDelay(180)
  if (
    !verifyW05CardClaim(
      input.workItemId,
      input.claimToken,
      input.leaseVersion
    )
  ) {
    throw new Error("LEASE_INVALID")
  }
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
