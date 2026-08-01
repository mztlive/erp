import { mockDelay } from "@/features/workspace-kit/delay"
import type {
  CardSalesApproval,
  ProcurementRejectionResolution,
  SalesOrderListItem,
} from "@/features/sales-orders/types"
import { MOCK_SALES_ORDERS } from "@/mock/sales-orders"
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
  hasW05DraftPriceAdjusted,
  markW05DraftPriceAdjusted,
  postSalesOrderAcceptance,
  resolveW05ProcurementRejection,
  startW05SalesChangeOrder,
  verifyW05CardClaim,
} from "@/mock/session-state"

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
            acceptanceReason: "照原条件申请低毛利承接（会话演示）",
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
              reason: "存在有效低毛利后继任务，不可并发作废。",
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
                    "草稿相对被驳回提交尚无改品/改价；请先调整后再重提。",
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
              reason: "须先领取运营审批任务。",
            },
            {
              action: "REJECT",
              reason: "须先领取运营审批任务。",
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
            "正式版本只读；商业变化须通过销售变更单并完成影响确认与财务复核。",
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

export async function fetchSalesOrders(): Promise<SalesOrderListItem[]> {
  await mockDelay()
  return MOCK_SALES_ORDERS.map(mergeSessionOverlay)
}

export async function fetchSalesOrderDetail(
  id: string
): Promise<SalesOrderDetailView | null> {
  await mockDelay()
  const base = MOCK_SALES_ORDERS.find((item) => item.id === id)
  if (!base) return null
  const order = mergeSessionOverlay(base)
  return {
    ...order,
    acceptance: getSalesOrderAcceptance(id),
    permissionVersion: PERMISSION_VERSION,
    sourceAsOf: "2026-03-29T12:00:00+08:00",
    queriedAt: new Date().toISOString(),
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
