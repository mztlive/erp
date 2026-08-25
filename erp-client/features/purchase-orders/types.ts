/** W08 采购单 · 客户端契约类型（对齐工作面文档 §5/§8）。 */

import type { StatusTone } from "@/components/ui/status-badge"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import type {
    WorkItemProcessingState,
    WorkItemStatus,
} from "@/features/work-items"

export type PurchaseType = "PHYSICAL" | "VIRTUAL" | "SERVICE"

export type FulfillmentResponsibility =
    | "WAREHOUSE"
    | "SUPPLIER_DIRECT"
    | "ELECTRONIC"
    | "SERVICE"

export type PurchaseOrderStatus =
    | "DRAFT"
    | "PENDING_REVIEW"
    | "EFFECTIVE"
    | "PARTIAL"
    | "COMPLETED"
    | "VOID"

export type PurchaseReviewStatus = "NONE" | "PENDING" | "APPROVED" | "REJECTED"

export type PurchaseReviewDomainAction = "APPROVE" | "REJECT"

export type PaymentGateState = "SATISFIED" | "BLOCKED" | "NOT_APPLICABLE"

export type PurchaseOrderStatusFilter = "all" | PurchaseOrderStatus

export type PurchaseOrderMetricFilter =
    | "all"
    | "pending_create"
    | "draft"
    | "review"
    | "fulfill"
    | "gate_blocked"

export const PO_METRIC_LABEL: Record<PurchaseOrderMetricFilter, string> = {
    all: "全部采购单",
    pending_create: "可建单依据",
    draft: "草稿",
    review: "审批中",
    fulfill: "待履约",
    gate_blocked: "先款门禁阻塞",
}

export const PO_STATUS_FILTER_LABEL: Record<PurchaseOrderStatusFilter, string> =
    {
        all: "全部状态",
        DRAFT: "草稿",
        PENDING_REVIEW: "审批中",
        EFFECTIVE: "已生效",
        PARTIAL: "部分执行",
        COMPLETED: "已完成",
        VOID: "已作废",
    }

type ActionBlocker = {
    action: string
    code: string
    message: string
}

type PurchaseOrderLineView = Readonly<{
    lineId: string
    lineType: "ITEM_SERVICE" | "LOGISTICS_FEE"
    procurementConfirmationLineId?: string
    itemName: string
    itemSku?: string
    quantity?: string
    unit?: string
    unitCostGross: string
    inputTaxRate: string
    /** 服务端按行舍入到分后的金额 */
    grossAmount: string
    netAmount: string
    taxAmount: string
    expectedDeliveryDate?: string
    logisticsFeeReason?: string
    salesAllocationLabel?: string
}>

type PrepaymentGateView = Readonly<{
    state: PaymentGateState
    message: string
    required: string
    allocated: string
    gap: string
    updatedAt: string
}>

type PayableSummaryView = Readonly<{
    payableOpenAmount: string
    paidAllocatedAmount: string
    purchaseInvoiceAllocatedAmount: string
}>

type FulfillmentSummaryView = Readonly<{
    progressLabel: string
    progressTone: StatusTone
    inboundQty: string
    shippedQty: string
    remainingQty: string
    note?: string
}>

type RelatedChangeView = Readonly<{
    changeId: string
    label: string
    statusLabel: string
    tone: StatusTone
    baseRevisionNo?: number
}>

export type PurchaseChangeOrderSummary = Readonly<{
    id: string
    purchaseOrderId: string
    statusLabel: string
    statusTone: StatusTone
    /** 服务端状态码；审批相位只读此值与审批投影，不按仓配影响推导。 */
    statusCode: string
    /** 乐观锁版本；提交审批必须携带。 */
    version: number
    reason: string
    baseRevisionId: string
    createdAt: string
    /** 统一只读审批结构。缺省表示列表行尚未补详情。 */
    approval?: DocumentApprovalView
}>

/** PurchaseReturnOrder 为 NO_APPROVAL：行投影不得携带审批区。 */
export type PurchaseReturnOrderRow = Readonly<{
    purchaseReturnOrderId: string
    purchaseReturnNo: string
    purchaseOrderId: string
    salesReturnCaseId?: string
    returnMode: string
    returnModeLabel: string
    status: string
    statusLabel: string
    statusTone: StatusTone
    version: number
    createdAt: string
    allowedActions: readonly string[]
}>

type WorkflowActionView = Readonly<{
    id: string
    actionLabel: string
    actorLabel: string
    at: string
    comment?: string
}>

export type PurchaseOrderListItem = Readonly<{
    purchaseOrderId: string
    purchaseNo?: string
    draftLabel?: string
    revisionNo?: number
    status: PurchaseOrderStatus
    statusLabel: string
    statusTone: StatusTone
    reviewStatus: PurchaseReviewStatus
    reviewLabel: string
    salesOrderId: string
    salesOrderNo: string
    supplierId: string
    supplierName: string
    purchaseType: PurchaseType
    fulfillmentResponsibility: FulfillmentResponsibility
    paymentTermCode: string
    paymentTermLabel: string
    ownerName: string
    /** 服务端汇总；无成本权限时为掩码标记 */
    grossAmount: string
    netAmount: string
    taxAmount: string
    costMasked: boolean
    paymentProgress: string
    invoiceProgress: string
    fulfillmentProgress: string
    paymentGate: PaymentGateState
    expectedDate?: string
    updatedAt: string
    allowedActions: readonly string[]
    actionBlockers: readonly ActionBlocker[]
}>

export type PurchaseOrderCenterView = Readonly<{
    identity: {
        purchaseOrderId: string
        purchaseNo?: string
        draftLabel?: string
        status: PurchaseOrderStatus
        statusLabel: string
        statusTone: StatusTone
        reviewStatus: PurchaseReviewStatus
        reviewLabel: string
        lockVersion: number
        currentSubmissionId?: string
        currentRevisionId?: string
        revisionNo?: number
        subjectHash?: string
    }
    header: {
        salesOrderId: string
        salesOrderNo: string
        supplierId: string
        supplierSnapshot: string
        purchaseType: PurchaseType
        fulfillmentResponsibility: FulfillmentResponsibility
        paymentTermCode: string
        paymentTermLabel: string
        ownerName: string
        submittedBy?: string
        submittedAt?: string
        expectedDate?: string
        creationBasisId?: string
    }
    progress: {
        payment: string
        invoice: string
        fulfillment: string
        prepaymentGate: PrepaymentGateView
    }
    currentContent: {
        source: "DRAFT" | "SUBMISSION" | "REVISION"
        version: number
        subjectHash?: string
        lines: readonly PurchaseOrderLineView[]
        totals: { gross: string; net: string; tax: string }
        costMasked: boolean
    }
    allocations: readonly {
        lineId: string
        salesOrderLineLabel: string
        allocatedQuantity: string
    }[]
    payableSummary?: PayableSummaryView
    fulfillmentSummary: FulfillmentSummaryView
    changes: readonly RelatedChangeView[]
    workflow: readonly WorkflowActionView[]
    allowedActions: readonly string[]
    actionBlockers: readonly ActionBlocker[]
    fieldVisibility: Record<string, "full" | "masked" | "hidden">
    /** 统一只读审批投影；缺省表示服务端未返回绑定。 */
    approval?: DocumentApprovalView
    /** 当前进行中的采购变更单；缺省表示无在途改单。 */
    activeChangeOrder?: PurchaseChangeOrderSummary | null
    /** 审核任务（仅待审核且存在提交时） */
    reviewWorkItem?: {
        workItemId: string
        workItemType: "PURCHASE_ORDER_REVIEW"
        taskVersion: string
        subjectVersion: string
        status: WorkItemStatus
        ownerRole: string
        ownerOrganizationId: string
        ownerUserId?: string
        processingState: WorkItemProcessingState
        domainAllowedActions: readonly PurchaseReviewDomainAction[]
        actionBlockers: readonly ActionBlocker[]
    }
}>

export type PurchaseCreationBasis = Readonly<{
    basisId: string
    workItemId: string
    salesOrderId: string
    salesOrderNo: string
    customerName: string
    contractNumber?: string
    salesOwnerName?: string
    salesOrderRevisionId: string
    supplierId: string
    supplierName: string
    purchaseType: PurchaseType
    fulfillmentResponsibility: FulfillmentResponsibility
    paymentTermCode: string
    paymentTermLabel: string
    /** 供应商经营类目；从付款条件快照拆出，未登记时为空。 */
    businessCategory?: string
    /** 可拆入本单的销售当前版本行 */
    lines: readonly {
        salesOrderLineId: string
        salesOrderRevisionLineId: string
        itemName: string
        itemSku?: string
        salesQuantity: string
        coveredQuantity: string
        remainingQuantity: string
        maxCreateQuantity: string
        unit: string
        unitCostGross: string
        inputTaxRate: string
        expectedDeliveryDate: string
        /** 销售对客户承诺的最晚交付日。 */
        salesDeliveryDeadline: string
        salesAllocationLabel: string
    }[]
    estimatedGross: string
    consumed: boolean
}>

export type SavePurchaseOrderDraftInput = {
    purchaseOrderId: string
    expectedLockVersion: number
    draftEditToken: string
    paymentTermCode: string
    lines: Array<{
        lineId: string
        lineType: "ITEM_SERVICE" | "LOGISTICS_FEE"
        quantity?: string
        unitCostGross?: string
        inputTaxRate: string
        logisticsFeeReason?: string
    }>
    idempotencyKey: string
}

export type VoidPurchaseOrderInput = {
    purchaseOrderId: string
    expectedLockVersion: number
    reason: string
    idempotencyKey: string
}

export type SubmitPurchaseOrderInput = {
    purchaseOrderId: string
    expectedLockVersion: number
    expectedDraftContentHash: string
    draftEditToken: string
    paymentTermCode: string
    lines: SavePurchaseOrderDraftInput["lines"]
    idempotencyKey: string
}

export type SubmitPurchaseOrderPayload = Omit<
    SubmitPurchaseOrderInput,
    "idempotencyKey"
>

export type ReviewPurchaseOrderInput = {
    workItemId: string
    expectedTaskVersion: string
    expectedSubjectVersion: string
    decision:
        | {
              purchaseOrderId: string
              submissionId: string
              expectedPurchaseOrderLockVersion: number
              reviewResult: "APPROVED"
              comment?: string
          }
        | {
              purchaseOrderId: string
              submissionId: string
              expectedPurchaseOrderLockVersion: number
              reviewResult: "REJECTED"
              reasonCode: string
              comment?: string
          }
    idempotencyKey: string
}

export type CreatePurchaseOrderFromBasisInput = {
    basisId: string
    workItemId: string
    purchaseType: PurchaseType
    paymentTermCode: string
    lines: Array<{
        salesOrderLineId: string
        quantity: string
        expectedDeliveryDate: string
    }>
    idempotencyKey: string
}

export type CreatePurchaseOrdersFromSourcingInput = {
    workItemId: string
    salesOrderId: string
    lines: Array<{
        salesOrderLineId: string
        basisId: string
        quantity: string
        expectedDeliveryDate: string
    }>
    idempotencyKey: string
}

export type CreatedPurchaseOrderDraft = {
    purchaseOrderId: string
    draftLabel: string
    lockVersion: number
}

export type FormalActionResponse<T = unknown> =
    | { status: "succeeded"; data: T; reference: string }
    | { status: "failed"; message: string; code: string }
    | { status: "unknown"; message: string; idempotencyKey: string }

export const PURCHASE_TYPE_LABEL: Record<PurchaseType, string> = {
    PHYSICAL: "实物",
    VIRTUAL: "虚拟",
    SERVICE: "线下服务",
}

export const FULFILLMENT_RESPONSIBILITY_LABEL: Record<
    FulfillmentResponsibility,
    string
> = {
    WAREHOUSE: "入仓",
    SUPPLIER_DIRECT: "供应商直发",
    ELECTRONIC: "电子交付",
    SERVICE: "线下服务",
}

export const PO_STATUS_LABEL: Record<PurchaseOrderStatus, string> = {
    DRAFT: "草稿",
    PENDING_REVIEW: "审批中",
    EFFECTIVE: "已生效",
    PARTIAL: "部分执行",
    COMPLETED: "已完成",
    VOID: "已作废",
}

export const PO_STATUS_TONE: Record<PurchaseOrderStatus, StatusTone> = {
    DRAFT: "neutral",
    PENDING_REVIEW: "warning",
    EFFECTIVE: "success",
    PARTIAL: "info",
    COMPLETED: "success",
    VOID: "neutral",
}

export const REVIEW_STATUS_LABEL: Record<PurchaseReviewStatus, string> = {
    NONE: "—",
    PENDING: "审批中",
    APPROVED: "已通过",
    REJECTED: "已驳回",
}

export const REJECT_REASON_LABEL: Record<string, string> = {
    COST_TAX: "成本/税率不符",
    FEE: "费用行问题",
    PAYMENT_TERM: "付款条件问题",
    SUPPLIER: "供应商资料问题",
    ALLOCATION: "销售分配错误",
    OTHER: "其它",
}

/** 付款条件可选全集（含履约语义后缀）；保存时直接使用所选 label，不做二次映射 */
export const PAYMENT_TERM_OPTIONS: readonly { value: string; label: string }[] =
    [
        { value: "PREPAY_100", label: "先款 100% 后履约" },
        { value: "PREPAY_50", label: "先款 50% 后直发" },
        { value: "PREPAY_30", label: "先款 30%" },
        { value: "POSTPAY_NET15", label: "货到 15 天" },
        { value: "POSTPAY_NET30", label: "货到 30 天" },
    ]
