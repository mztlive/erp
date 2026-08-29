/** W06 客户验收 — client contract types */

export type FulfillmentFactType =
    | "WAREHOUSE_SHIP"
    | "SUPPLIER_DIRECT"
    | "ELECTRONIC"
    | "SERVICE"

export type AcceptanceOverallResult =
    | "PASS"
    | "SHORT"
    | "REJECT"
    | "SERVICE_FAIL"

export type AcceptanceStatus = "DRAFT" | "POSTED" | "REVERSED"

type AllocationDirection = "APPLY" | "REVERSE"

export type AcceptanceEligibleFact = {
    fulfillmentLineId: string
    fulfillmentFactType: FulfillmentFactType
    fulfillmentNo: string
    salesOrderLineId: string
    lineNo: number
    itemSnapshot: string
    unitCode: string
    occurredAt: string
    /** 服务端扣除冲正后的有效履约数量 */
    netSuccessfulQuantity: string
    /** 服务端 APPLY − REVERSE 净分配 */
    netAcceptedAllocatedQuantity: string
    /** 服务端守恒：本次最多可验收 */
    eligibleQuantity: string
    carrier?: string
    trackingNo?: string
}

type AcceptanceAllocationRecord = {
    fulfillmentLineId: string
    fulfillmentNo: string
    fulfillmentFactType: FulfillmentFactType
    salesOrderLineId: string
    direction: AllocationDirection
    allocatedQuantity: string
}

type AcceptanceLineRecord = {
    salesOrderLineId: string
    lineNo: number
    itemSnapshot: string
    unitCode: string
    acceptedQuantity: string
    shortQuantity: string
    rejectedQuantity: string
    reason?: string
    allocations: AcceptanceAllocationRecord[]
}

export type AcceptanceHistoryItem = {
    acceptanceId: string
    acceptanceNo: string
    status: Extract<AcceptanceStatus, "POSTED" | "REVERSED">
    acceptedAt: string
    postedAt: string
    overallResult: AcceptanceOverallResult
    lines: AcceptanceLineRecord[]
    recordedBy: string
    version: number
    comment?: string
    reversalOfAcceptanceId?: string
    reversedByAcceptanceId?: string
    /** 结果区文案：仅记录验收记录，不暗示库存/票款 */
    factOnlyNotice: string
}

export type AcceptanceDraftLine = {
    salesOrderLineId: string
    acceptedQuantity: string
    shortQuantity: string
    rejectedQuantity: string
    reason: string
    /** 服务不通过：写入草稿与提交 payload，草稿恢复时原样还原。 */
    serviceFail?: boolean
    allocations: Array<{
        fulfillmentLineId: string
        fulfillmentFactType: FulfillmentFactType
        allocatedQuantity: string
    }>
}

type AcceptanceDraft = {
    acceptanceDraftId: string
    draftVersion: number
    salesOrderId: string
    acceptedAt: string
    comment: string
    lines: AcceptanceDraftLine[]
    updatedAt: string
}

export type AcceptanceSalesLineGroup = {
    salesOrderLineId: string
    lineNo: number
    itemSnapshot: string
    unitCode: string
    requiredQuantity: string
    /** 服务端净已验收 */
    netAcceptedQuantity: string
    fulfillmentFacts: AcceptanceEligibleFact[]
}

export type CustomerAcceptanceWorkspaceView = {
    salesOrder: {
        id: string
        salesOrderNo: string
        businessType: "GOODS_SERVICE" | "CARD_VOUCHER"
        customerLabel: string
        commercialStatus: string
        commercialStatusTone: string
        fulfillmentProgress: string
        collectionProgress: string
        invoiceProgress: string
        lockVersion: number
        factsUpdatedAt: string
    }
    freshness: {
        factsUpdatedAt: string
        state: "fresh" | "syncing" | "failed"
    }
    metrics: {
        eligibleFulfillmentCount: number
        eligibleQuantityByUnit: Array<{ unitCode: string; quantity: string }>
        overdueLineCount: number
    }
    salesLines: AcceptanceSalesLineGroup[]
    draft: AcceptanceDraft | null
    history: AcceptanceHistoryItem[]
    permissions: {
        allowedActions: string[]
        actionBlockers: Array<{ action: string; code: string; message: string }>
        fieldVisibility: Record<string, "full" | "masked" | "hidden">
    }
    /** 从统一工作台进入时已由前端与服务端共同校验的正式任务身份。 */
    workItem: {
        id: string
        expectedTaskVersion: number
    } | null
    /** 工作项身份、处理器或状态不满足 W06 合同时的阻断（fail-closed）。 */
    workItemConfigBlocker: string | null
}

export type PostAcceptanceInput = {
    workItemId?: string
    expectedTaskVersion?: number
    salesOrderId: string
    acceptanceDraftId: string
    expectedDraftVersion: number
    expectedSalesOrderLockVersion: number
    idempotencyKey: string
    acceptedAt: string
    comment: string
    lines: AcceptanceDraftLine[]
}

export type PostAcceptanceResult =
    | {
          status: "succeeded"
          acceptanceNo: string
          acceptanceId: string
          remainingEligibleCount: number
          remainingEligibleQuantityLabel: string
          overallResult: AcceptanceOverallResult
          factOnlyNotice: string
      }
    | {
          status: "unknown"
          message: string
          idempotencyKey: string
      }
    | {
          status: "failed"
          message: string
      }

export type ReverseAcceptanceInput = {
    salesOrderId: string
    acceptanceId: string
    expectedAcceptanceVersion: number
    reasonText: string
    idempotencyKey: string
}

export type ReverseAcceptanceResult =
    | {
          status: "succeeded"
          reverseAcceptanceNo: string
          reverseAcceptanceId: string
          originalAcceptanceNo: string
      }
    | { status: "failed"; message: string }

export type SaveAcceptanceDraftInput = {
    salesOrderId: string
    acceptanceDraftId?: string
    expectedDraftVersion?: number
    acceptedAt: string
    comment: string
    lines: AcceptanceDraftLine[]
}

export const FULFILLMENT_TYPE_LABEL: Record<FulfillmentFactType, string> = {
    WAREHOUSE_SHIP: "仓发",
    SUPPLIER_DIRECT: "代发",
    ELECTRONIC: "电子交付",
    SERVICE: "服务履约",
}

export const OVERALL_RESULT_LABEL: Record<AcceptanceOverallResult, string> = {
    PASS: "通过",
    SHORT: "短少",
    REJECT: "拒收",
    SERVICE_FAIL: "服务不通过",
}

export const FACT_ONLY_NOTICE =
    "短少、拒收或服务不通过只记客户结果，不会自动退货、退款或改应收。请另开退货或拒收处理单。"
