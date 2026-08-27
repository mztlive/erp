/**
 * W25 商城消费订单 · 客户端契约
 * 声明式数据豁免：本文件全部为类型声明与枚举中文映射表（const 配置数据），
 * 无逻辑与组件，超过 400 行也按声明式数据表豁免拆分。
 */

import type { StatusTone } from "@/components/ui/status-badge"

export type FulfillmentChain = "LEGACY_MANUAL" | "ERP_AUTOMATED"

export type AttributionStatus = "ATTRIBUTED" | "PENDING" | "DIFFERENCE"

export type CostBasis = "ACTUAL" | "STANDARD" | "NONE"

type PaymentSourceType = "CARD" | "WECHAT"

export type PaymentSourceFilter = PaymentSourceType | "MIXED"

export type DataSource = "REALTIME" | "BACKFILL" | "MIXED"

export type ProcessingStatus =
    | "SAVED"
    | "PENDING_ATTRIBUTION"
    | "ATTRIBUTED"
    | "DIFFERENCE"
    | "REJECTED"

export type FactType =
    | "PAYMENT_SUCCEEDED"
    | "ORDER_CANCELED"
    | "REFUND_SUCCEEDED"
    | "ORDER_COMPLETED"
    | "CARD_BALANCE_RESTORED"

export type SupplierFulfillmentStatus =
    | "RECEIVED"
    | "SUBMITTING"
    | "ACCEPTED"
    | "REJECTED"
    | "RESULT_UNKNOWN"
    | "FULFILLING"
    | "SHIPPED"
    | "COMPLETED"
    | "EXCEPTION"

export type EmptyReason =
    | "NO_DATA"
    | "FILTER_EMPTY"
    | "NO_SCOPE"
    | "NO_PERMISSION"

export const FULFILLMENT_CHAIN_LABEL: Record<FulfillmentChain, string> = {
    LEGACY_MANUAL: "原人工履约",
    ERP_AUTOMATED: "ERP 自动履约",
}

export const FULFILLMENT_CHAIN_TONE: Record<FulfillmentChain, StatusTone> = {
    LEGACY_MANUAL: "neutral",
    ERP_AUTOMATED: "info",
}

export const ATTRIBUTION_STATUS_LABEL: Record<AttributionStatus, string> = {
    ATTRIBUTED: "已归集",
    PENDING: "待归集",
    DIFFERENCE: "差异",
}

export const PROCESSING_STATUS_LABEL: Record<ProcessingStatus, string> = {
    SAVED: "已保存",
    PENDING_ATTRIBUTION: "待归集",
    ATTRIBUTED: "已归集",
    DIFFERENCE: "差异",
    REJECTED: "拒绝",
}

export const ATTRIBUTION_STATUS_TONE: Record<AttributionStatus, StatusTone> = {
    ATTRIBUTED: "success",
    PENDING: "warning",
    DIFFERENCE: "destructive",
}

export const COST_BASIS_LABEL: Record<CostBasis, string> = {
    ACTUAL: "实际成本",
    STANDARD: "标准成本",
    NONE: "无成本",
}

export const COST_BASIS_TONE: Record<CostBasis, StatusTone> = {
    ACTUAL: "success",
    STANDARD: "info",
    NONE: "warning",
}

/** 供应商子订单取消状态中文名（枚举原值禁止上屏；与供应商订单口径一致） */
export const SUPPLIER_CANCEL_LABEL: Record<string, string> = {
    NONE: "未发起",
    CANCEL_PENDING: "处理中",
    CANCELED: "已取消",
    FAILED: "失败",
    MANUAL: "待人工",
}

/** 供应商子订单退款状态中文名（枚举原值禁止上屏；与供应商订单口径一致） */
export const SUPPLIER_REFUND_LABEL: Record<string, string> = {
    NONE: "未发起",
    REFUND_PENDING: "处理中",
    PARTIAL: "部分",
    REFUNDED: "全部",
    REFUND_FAILED: "失败",
    MANUAL: "待人工",
}

export const FACT_TYPE_LABEL: Record<FactType, string> = {
    PAYMENT_SUCCEEDED: "支付成功",
    ORDER_CANCELED: "订单已取消",
    REFUND_SUCCEEDED: "商城退款成功",
    ORDER_COMPLETED: "商城订单已完成",
    CARD_BALANCE_RESTORED: "卡券余额已恢复",
}

export const FACT_TYPE_TONE: Record<FactType, StatusTone> = {
    PAYMENT_SUCCEEDED: "success",
    ORDER_CANCELED: "neutral",
    REFUND_SUCCEEDED: "warning",
    ORDER_COMPLETED: "info",
    CARD_BALANCE_RESTORED: "info",
}

export const SUPPLIER_STATUS_LABEL: Record<SupplierFulfillmentStatus, string> =
    {
        RECEIVED: "已接收",
        SUBMITTING: "下单中",
        ACCEPTED: "已接单",
        REJECTED: "已拒单",
        RESULT_UNKNOWN: "结果未知",
        FULFILLING: "履约中",
        SHIPPED: "已发货",
        COMPLETED: "已完成",
        EXCEPTION: "异常",
    }

export const DATA_SOURCE_LABEL: Record<DataSource, string> = {
    REALTIME: "实时",
    BACKFILL: "历史回填",
    MIXED: "混合",
}

export const PAYMENT_SOURCE_LABEL: Record<PaymentSourceFilter, string> = {
    CARD: "卡券",
    WECHAT: "微信",
    MIXED: "组合",
}

type ActionBlocker = {
    action: string
    code: string
    message: string
}

export type MallConsumptionOrderListQuery = {
    q?: string
    mallIds?: string[]
    /**
     * 记录发生期间（按记录发生时间 occurredAt 过滤，非 ERP 接收时间）。
     * 角色默认期间策略未配置时不预填：必须由用户显式选择完整起止时间后才允许查询，
     * 不静默回退到任意默认期间。
     */
    occurredFrom?: string
    occurredTo?: string
    factTypes?: FactType[]
    fulfillmentChains?: FulfillmentChain[]
    attributionStatuses?: AttributionStatus[]
    paymentSources?: PaymentSourceFilter[]
    supplierStatuses?: SupplierFulfillmentStatus[]
    costBases?: CostBasis[]
    dataSources?: Array<Exclude<DataSource, "MIXED">>
    /** 指标快捷：paid | pending_attr | fact_diff | auto_exception | cost_none */
    metric?: string
    sort?: string
    page?: number
    pageSize?: number
}

/** W05 卡券销售单只读协同摘要；不参与销售单关闭条件。 */
export type SalesOrderConsumptionSummary = {
    salesOrderId: string
    orderCount: number
    paidAmount: string
    refundedAmount: string
    restoredBalanceAmount: string
    latestFactAt?: string
}

type PaymentComposition = {
    cardAmount: string
    wechatAmount: string
    sourceCount: number
}

type FactSummaryItem = {
    factType: FactType
    latestOccurredAt: string
    count: number
}

type CostBasisBreakdownItem = {
    basis: CostBasis
    lineCount: number
    /** NONE 时省略，不展示 0 */
    costAmount?: string
}

type SupplierOrderSummary = {
    total: number
    statuses: string[]
    hasException: boolean
}

export type MallConsumptionOrderRow = {
    mallOrderId: string
    mallId: string
    mallName: string
    externalOrderNo: string
    customerId?: string
    customerLabel: string
    paidAt: string
    paidAmount: string
    paymentComposition: PaymentComposition
    factSummary: FactSummaryItem[]
    fulfillmentChain: FulfillmentChain
    supplierOrderSummary: SupplierOrderSummary
    attributionStatus: AttributionStatus
    costBasisBreakdown: CostBasisBreakdownItem[]
    dataSource: DataSource
    allowedActions: string[]
    /** 命中 W29 错误任务/对账差异时携带的稳定 work item（钻取入口用） */
    workItemId?: string
    actionBlockers: ActionBlocker[]
    costBasisPolicyState: "CONFIGURED" | "UNCONFIGURED"
    normalizedCostBasis?: CostBasis | "MIXED"
}

export type MallConsumptionOrderMetricKey =
    | "paid"
    | "pending_attr"
    | "fact_diff"
    | "auto_exception"
    | "cost_none"

export const MALL_CONSUMPTION_METRIC_LABELS: Record<
    MallConsumptionOrderMetricKey,
    string
> = {
    paid: "支付成功",
    pending_attr: "待归集",
    fact_diff: "记录差异",
    auto_exception: "自动履约异常",
    cost_none: "成本未覆盖",
}

export type MallConsumptionOrderMetric = {
    key: MallConsumptionOrderMetricKey
    label: string
    value: number
    detail?: string
}

export type MallConsumptionOrderListResult = {
    rows: MallConsumptionOrderRow[]
    pageInfo: { page: number; pageSize: number; total: number }
    metrics: MallConsumptionOrderMetric[]
    malls: Array<{ id: string; name: string }>
    filterSummary: string
    emptyReason?: EmptyReason
    hasModulePermission: boolean
    hasDataScope: boolean
    permissionVersion: string
    dataScopeVersion: string
    factWatermark: string
    queriedAt: string
    /** 记录追溯只读边界说明 */
    boundaryNotice: string
}

export type MallOrderFactView = {
    factId: string
    factType: FactType
    businessFactKeySummary: string
    externalOrderVersion: string
    afterSalesRequestId?: string
    originalPaymentFactId?: string
    occurredAt: string
    receivedAt: string
    dataSource: "REALTIME" | "BACKFILL"
    processingStatus: ProcessingStatus
    resultDetails: Record<string, string | number | null>
}

type MallOrderItemView = {
    mallOrderItemId: string
    externalItemId: string
    skuId?: string
    productPublicationRevisionId?: string
    supplierOfferingRevisionId?: string
    nameSnapshot: string
    specSnapshot: string
    quantity: string
    unitPriceGross: string
    lineGrossAmount: string
    allocatedDiscountAmount: string
    allocatedFreightAmount: string
    paidAmount: string
    salesTaxRate: string
    unitCostSnapshot?: string
    costSnapshotTotal?: string
    costTaxInclusion?: string
    costInputTaxRate?: string
    attributionStatus: AttributionStatus
}

type CostAssessmentView = {
    assessmentId: string
    assessmentNo: number
    costBasis: CostBasis
    basisSourceLabel: string
    /** NONE 时为空，不展示 0 */
    grossAmount?: string
    netAmount?: string
    taxAmount?: string
    taxInclusion?: string
    inputTaxRate?: string
    assessedAt: string
    noneReason?: string
}

export type PaymentSourceView = {
    paymentSourceId: string
    sourceNo: number
    sourceType: PaymentSourceType
    amount: string
    /** 短引用；卡实例标注非卡号 */
    sourceReference: string
    mallCardInstanceId?: string
    attributionStatus: AttributionStatus
    attributionIssue?: {
        type: "SOURCE_OBJECT_MISSING" | "UNATTRIBUTED" | "BASELINE_CONFLICT"
        ownerRole: "OPERATIONS" | "FINANCE"
        workItemId?: string
        correctionId?: string
    }
    origin?: {
        customerId: string
        customerLabel: string
        salesOrderId: string
        salesOrderNo: string
        salesOrderLineId: string
    }
}

type FundingAllocation = {
    mallOrderItemId: string
    paymentSourceId: string
    allocatedPaymentAmount: string
}

type ConservationResult = {
    itemRowResults: Array<{
        mallOrderItemId: string
        expected: string
        actual: string
        valid: boolean
    }>
    sourceColumnResults: Array<{
        paymentSourceId: string
        expected: string
        actual: string
        valid: boolean
    }>
    orderTotal: { expected: string; actual: string; valid: boolean }
}

type ConsumptionEntryView = {
    consumptionEntryId: string
    factId: string
    itemId: string
    paymentSourceId: string
    direction: "CONSUMPTION" | "REVERSAL"
    amount: string
    occurredAt: string
    attributionStatus: AttributionStatus
    originSalesOrderId?: string
    reversesConsumptionEntryId?: string
    currentCostAssessment: CostAssessmentView
}

type SupplierOrderView = {
    supplierFulfillmentOrderId: string
    fulfillmentOrderNo: string
    supplierLabel: string
    itemIds: string[]
    fulfillmentStatus: SupplierFulfillmentStatus
    cancelStatus: string
    refundStatus: string
    supplierRefundSummary?: {
        refundFactCount: number
        costReductionGross: string
        payableReductionGross: string
        cashRefundGross: string
        reversedPaymentAllocationCount: number
    }
}

export type MallConsumptionOrderView = {
    identity: {
        mallOrderId: string
        mallId: string
        mallName: string
        externalOrderNo: string
        paymentFactId: string
    }
    customer: {
        sourceCustomerRef: string
        customerId?: string
        customerLabel: string
        attributionStatus: AttributionStatus
    }
    orderedAt: string
    paidAt: string
    amounts: {
        gross: string
        discount: string
        freight: string
        paid: string
        conservationStatus: "VALID" | "DIFFERENCE"
    }
    fulfillment: {
        chain: FulfillmentChain
        cutoverId: string
        cutoverAt: string
        decidedByOccurredAt: string
        /** T 后供给不足等阻断说明 */
        autoFulfillmentBlocker?: string
    }
    facts: MallOrderFactView[]
    items: MallOrderItemView[]
    paymentSources: PaymentSourceView[]
    fundingAllocations: FundingAllocation[]
    conservation: ConservationResult
    consumptionEntries: ConsumptionEntryView[]
    supplierOrders: SupplierOrderView[]
    address: { maskedSummary: string; revealAllowed: boolean }
    phoneMasked: string
    paymentRefMasked: string
    freshness: {
        factWatermark: string
        attributionUpdatedAt: string
        supplierUpdatedAt?: string
        costAssessedAt?: string
        queriedAt: string
    }
    allowedActions: string[]
    actionBlockers: ActionBlocker[]
    fieldPermissions: Record<string, "full" | "masked" | "hidden">
    /** 支付已发生、履约/归集异常时展示 */
    paymentOccurredAlert?: {
        title: string
        message: string
        severity: "warning" | "destructive"
    }
    boundaryNotice: string
    workItemIds: string[]
}

export type ExportCommand = {
    selectionSnapshotId: string
    fieldSetId: string
    requestId: string
    rowCount: number
    filterSummary: string
}

/** 导出任务创建成功后的页内结果展示状态。 */
export type ExportResultState = {
    jobId: string
    rowCount: number
    permissionVersion: string
    maskDisclaimer: string
    downloadLabel: string
    expiresAt: string
}

export type ExportJobResult = {
    jobId: string
    requestId: string
    rowCount: number
    permissionVersion: string
    fieldSetId: string
    maskDisclaimer: string
    expiresAt: string
    downloadLabel: string
    status: "queued" | "succeeded"
}

export type ObjectCenterSectionId =
    | "overview"
    | "facts"
    | "items"
    | "payment"
    | "origin"
    | "supplier"
    | "cost"
    | "aftersales"
    | "audit"

export const OBJECT_CENTER_SECTIONS: Array<{
    id: ObjectCenterSectionId
    label: string
}> = [
    { id: "overview", label: "概览" },
    { id: "facts", label: "关键记录" },
    { id: "items", label: "商品明细" },
    { id: "payment", label: "支付与分摊" },
    { id: "origin", label: "来源追溯" },
    { id: "supplier", label: "供应商履约" },
    { id: "cost", label: "成本口径" },
    { id: "aftersales", label: "售后结果" },
    { id: "audit", label: "审计" },
]
