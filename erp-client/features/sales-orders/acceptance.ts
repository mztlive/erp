/**
 * W06 客户验收 API（queryFn / mutationFn）。
 * 兼容 re-export 旧 detail/acceptance 入口；新 UI 使用 workspace 系列。
 */

import { mockDelay } from "@/lib/mock-delay"
import type {
  AcceptanceEligibleFact,
  AcceptanceSalesLineGroup,
  CustomerAcceptanceWorkspaceView,
  PostAcceptanceInput,
  PostAcceptanceResult,
  ReverseAcceptanceInput,
  ReverseAcceptanceResult,
  SaveAcceptanceDraftInput,
} from "@/features/sales-orders/acceptance-types"
import {
  clearAcceptanceDraft,
  getAcceptanceDraft,
  getEligibleQuantity,
  getNetAcceptedAllocated,
  isAcceptancePermissionRevoked,
  listAcceptanceHistory,
  postCustomerAcceptance,
  restoreAcceptancePermission,
  reverseCustomerAcceptance,
  revokeAcceptancePermission,
  saveAcceptanceDraft,
} from "@/mock/session-state"
import { listBaselineFactsForOrder } from "@/mock/acceptance-fulfillment"
import { MOCK_SALES_ORDERS } from "@/mock/sales-orders"

export {
  fetchSalesOrderDetail,
  submitSalesOrderAcceptance,
  type SalesOrderDetailView,
} from "@/features/sales-orders/api"

function formatQty(n: number): string {
  if (Number.isInteger(n)) return String(n)
  return n.toFixed(6).replace(/\.?0+$/, "")
}

function buildEligibleFacts(salesOrderId: string): AcceptanceEligibleFact[] {
  return listBaselineFactsForOrder(salesOrderId).map((base) => {
    const netAccepted = getNetAcceptedAllocated(
      salesOrderId,
      base.fulfillmentLineId
    )
    const eligible = getEligibleQuantity(salesOrderId, base.fulfillmentLineId)
    return {
      fulfillmentLineId: base.fulfillmentLineId,
      fulfillmentFactType: base.fulfillmentFactType,
      fulfillmentNo: base.fulfillmentNo,
      salesOrderLineId: base.salesOrderLineId,
      lineNo: base.lineNo,
      itemSnapshot: base.itemSnapshot,
      unitCode: base.unitCode,
      occurredAt: base.occurredAt,
      netSuccessfulQuantity: base.netSuccessfulQuantity,
      netAcceptedAllocatedQuantity: formatQty(netAccepted),
      eligibleQuantity: formatQty(eligible),
      carrier: base.carrier,
      trackingNo: base.trackingNo,
    }
  })
}

function groupSalesLines(
  order: (typeof MOCK_SALES_ORDERS)[number],
  facts: AcceptanceEligibleFact[]
): AcceptanceSalesLineGroup[] {
  const byLine = new Map<string, AcceptanceEligibleFact[]>()
  for (const fact of facts) {
    const list = byLine.get(fact.salesOrderLineId) ?? []
    list.push(fact)
    byLine.set(fact.salesOrderLineId, list)
  }

  return order.lineItems.map((line, index) => {
    const lineFacts = (byLine.get(line.id) ?? []).sort((a, b) =>
      a.occurredAt.localeCompare(b.occurredAt)
    )
    const netAccepted = lineFacts.reduce(
      (sum, f) => sum + Number(f.netAcceptedAllocatedQuantity),
      0
    )
    return {
      salesOrderLineId: line.id,
      lineNo: index + 1,
      itemSnapshot: `${line.name}${line.sku ? ` · ${line.sku}` : ""}`,
      unitCode: line.unit,
      requiredQuantity: line.quantity,
      netAcceptedQuantity: formatQty(netAccepted),
      fulfillmentFacts: lineFacts,
    }
  })
}

export type FetchAcceptanceWorkspaceParams = {
  salesOrderId: string
  remainingOnly?: boolean
  workItemId?: string | null
}

export async function fetchCustomerAcceptanceWorkspace(
  params: FetchAcceptanceWorkspaceParams
): Promise<CustomerAcceptanceWorkspaceView | null> {
  await mockDelay()
  const order = MOCK_SALES_ORDERS.find((item) => item.id === params.salesOrderId)
  if (!order) return null

  const permissionRevoked = isAcceptancePermissionRevoked()
  const workItemConfigBlocker = params.workItemId
    ? "客户验收任务类型尚未注册。请从销售单直接登记验收，不要使用待办队列入口。"
    : null

  if (permissionRevoked) {
    return {
      salesOrder: {
        id: order.id,
        salesOrderNo: order.documentNumber,
        businessType:
          order.nature === "card_voucher" ? "CARD_VOUCHER" : "GOODS_SERVICE",
        customerLabel: "（权限已收回，已清除）",
        commercialStatus: order.primaryStatus.label,
        commercialStatusTone: order.primaryStatus.tone,
        fulfillmentProgress: order.fulfillment.label,
        collectionProgress: order.collection.label,
        invoiceProgress: order.invoicing.label,
        lockVersion: order.lockVersion ?? order.version,
        factsUpdatedAt: new Date().toISOString(),
      },
      freshness: {
        factsUpdatedAt: new Date().toISOString(),
        state: "fresh",
      },
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
            action: "POST_ACCEPTANCE",
            code: "PERMISSION_REVOKED",
            message:
              "操作期间权限被收回：已停止自动保存与提交，敏感数据已清理",
          },
          {
            action: "CREATE_ACCEPTANCE",
            code: "PERMISSION_REVOKED",
            message: "无验收写权限",
          },
        ],
        fieldVisibility: {
          customerName: "hidden",
          customerContact: "hidden",
          deliveryAddress: "hidden",
        },
      },
      workItem: null,
      lease: null,
      workItemConfigBlocker,
    }
  }

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
        factsUpdatedAt: new Date().toISOString(),
      },
      freshness: {
        factsUpdatedAt: new Date().toISOString(),
        state: "fresh",
      },
      metrics: {
        eligibleFulfillmentCount: 0,
        eligibleQuantityByUnit: [],
        overdueLineCount: 0,
      },
      salesLines: [],
      draft: null,
      history: listAcceptanceHistory(order.id),
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

  const allFacts = buildEligibleFacts(order.id)
  const remainingOnly = params.remainingOnly !== false
  const factsForDisplay = remainingOnly
    ? allFacts.filter((f) => Number(f.eligibleQuantity) > 0)
    : allFacts

  const salesLines = groupSalesLines(order, factsForDisplay)

  const eligibleFacts = allFacts.filter((f) => Number(f.eligibleQuantity) > 0)
  const qtyByUnit = new Map<string, number>()
  for (const fact of eligibleFacts) {
    qtyByUnit.set(
      fact.unitCode,
      (qtyByUnit.get(fact.unitCode) ?? 0) + Number(fact.eligibleQuantity)
    )
  }

  const allowedActions = [
    "CREATE_ACCEPTANCE",
    "POST_ACCEPTANCE",
    "SAVE_DRAFT",
  ]
  if (listAcceptanceHistory(order.id).some((h) => h.status === "POSTED")) {
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
      factsUpdatedAt: new Date().toISOString(),
    },
    freshness: {
      factsUpdatedAt: new Date().toISOString(),
      state: "fresh",
    },
    metrics: {
      eligibleFulfillmentCount: eligibleFacts.length,
      eligibleQuantityByUnit: [...qtyByUnit.entries()].map(
        ([unitCode, quantity]) => ({
          unitCode,
          quantity: formatQty(quantity),
        })
      ),
      overdueLineCount: 0,
    },
    salesLines,
    draft: getAcceptanceDraft(order.id),
    history: listAcceptanceHistory(order.id),
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
  await mockDelay(100)
  return saveAcceptanceDraft(input)
}

export async function postCustomerAcceptanceWorkspace(
  input: PostAcceptanceInput
): Promise<PostAcceptanceResult> {
  await mockDelay(180)
  return postCustomerAcceptance(input)
}

export async function reverseCustomerAcceptanceWorkspace(
  input: ReverseAcceptanceInput
): Promise<ReverseAcceptanceResult> {
  await mockDelay(180)
  return reverseCustomerAcceptance(input)
}

export async function clearCustomerAcceptanceDraft(salesOrderId: string) {
  await mockDelay(40)
  clearAcceptanceDraft(salesOrderId)
}

export function demoRevokeAcceptancePermission() {
  revokeAcceptancePermission()
}

export function demoRestoreAcceptancePermission() {
  restoreAcceptancePermission()
}
