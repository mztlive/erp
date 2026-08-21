/**
 * W25 商城消费订单 · 筛选参数清单与派生规则
 * 对齐 docs/ui-filter-design.md §5.6：筛选参数集中声明，
 * hasAppliedFilters / chips / 提交与清除共用同一清单，避免漏清或隐形状态。
 *
 * 期间（occurredFrom / occurredTo）是本页的分析期间维度：
 * 「清空全部」与「重置更多条件」都保留期间，只清理可移除的筛选参数。
 */

import type {
    AttributionStatus,
    CostBasis,
    DataSource,
    FactType,
    FulfillmentChain,
    MallConsumptionOrderMetricKey,
    PaymentSourceFilter,
    SupplierFulfillmentStatus,
} from "@/features/mall-consumption-orders/types"
import {
    ATTRIBUTION_STATUS_LABEL,
    COST_BASIS_LABEL,
    DATA_SOURCE_LABEL,
    FACT_TYPE_LABEL,
    FULFILLMENT_CHAIN_LABEL,
    MALL_CONSUMPTION_METRIC_LABELS,
    PAYMENT_SOURCE_LABEL,
    SUPPLIER_STATUS_LABEL,
} from "@/features/mall-consumption-orders/types"
import {
    DATA_SOURCES,
    FACT_TYPES,
    SUPPLIER_STATUSES,
} from "@/features/mall-consumption-orders/lib/url-state"

/** 可被单独移除的已生效条件（chip 维度；期间不在其中）。 */
export type MallConsumptionOrderFilterKey =
    | "q"
    | "mall"
    | "attributionStatus"
    | "fulfillmentChain"
    | "paymentSource"
    | "costBasis"
    | "factTypes"
    | "supplierStatuses"
    | "dataSources"
    | "metric"

/** 筛选参数（chip 与「清空全部」共用同一清单；期间与分页单独处理）。 */
export const MALL_CONSUMPTION_FILTER_PARAM_KEYS = [
    "q",
    "mall",
    "attributionStatus",
    "fulfillmentChain",
    "paymentSource",
    "costBasis",
    "factType",
    "supplierStatus",
    "dataSource",
    "metric",
] as const

export type MallConsumptionOrderApplied = {
    q: string
    /** "all" 表示未选商城。 */
    mallId: string
    attributionStatus: AttributionStatus | "all"
    fulfillmentChain: FulfillmentChain | "all"
    paymentSource: PaymentSourceFilter | "all"
    costBasis: CostBasis | "all"
    factTypes: FactType[]
    supplierStatuses: SupplierFulfillmentStatus[]
    dataSources: Array<Exclude<DataSource, "MIXED">>
    occurredFrom: string
    occurredTo: string
    metric: MallConsumptionOrderMetricKey | "all"
}

/** 面板内全部字段的受控草稿；"all"/空串/空数组表示未选。 */
export type MallConsumptionOrderFilterDraft = {
    mallId: string
    attributionStatus: AttributionStatus | "all"
    fulfillmentChain: FulfillmentChain | "all"
    paymentSource: PaymentSourceFilter | "all"
    costBasis: CostBasis | "all"
    factTypes: FactType[]
    supplierStatuses: SupplierFulfillmentStatus[]
    dataSources: Array<Exclude<DataSource, "MIXED">>
    occurredFrom: string
    occurredTo: string
}

export const EMPTY_MALL_CONSUMPTION_ORDER_FILTER_DRAFT: MallConsumptionOrderFilterDraft =
    {
        mallId: "",
        attributionStatus: "all",
        fulfillmentChain: "all",
        paymentSource: "all",
        costBasis: "all",
        factTypes: [],
        supplierStatuses: [],
        dataSources: [],
        occurredFrom: "",
        occurredTo: "",
    }

export function toMallConsumptionFilterDraft(
    applied: MallConsumptionOrderApplied,
): MallConsumptionOrderFilterDraft {
    return {
        mallId: applied.mallId === "all" ? "" : applied.mallId,
        attributionStatus: applied.attributionStatus,
        fulfillmentChain: applied.fulfillmentChain,
        paymentSource: applied.paymentSource,
        costBasis: applied.costBasis,
        factTypes: [...applied.factTypes],
        supplierStatuses: [...applied.supplierStatuses],
        dataSources: [...applied.dataSources],
        occurredFrom: applied.occurredFrom,
        occurredTo: applied.occurredTo,
    }
}

/** 结构化条件（面板内字段）是否存在已生效值；期间 / 指标 / 关键词不计入。 */
export function hasStructuredMallConsumptionFilters(
    applied: MallConsumptionOrderApplied,
): boolean {
    return Boolean(
        applied.mallId !== "all" ||
            applied.attributionStatus !== "all" ||
            applied.fulfillmentChain !== "all" ||
            applied.paymentSource !== "all" ||
            applied.costBasis !== "all" ||
            applied.factTypes.length > 0 ||
            applied.supplierStatuses.length > 0 ||
            applied.dataSources.length > 0,
    )
}

/** 全部筛选参数（含关键词与指标）是否存在已生效值；期间是分析维度，不计入。 */
export function hasAppliedMallConsumptionFilters(
    applied: MallConsumptionOrderApplied,
): boolean {
    return (
        hasStructuredMallConsumptionFilters(applied) ||
        Boolean(applied.q.trim()) ||
        applied.metric !== "all"
    )
}

export type MallConsumptionAppliedChip = Readonly<{
    key: MallConsumptionOrderFilterKey
    label: string
}>

/** 全部已生效条件均可单独撤销；chip 展示业务名或中文映射，不展示内部枚举原值。 */
export function buildMallConsumptionFilterChips(
    applied: MallConsumptionOrderApplied,
    malls: readonly { id: string; name: string }[],
): readonly MallConsumptionAppliedChip[] {
    const chips: MallConsumptionAppliedChip[] = []
    const q = applied.q.trim()
    if (q) chips.push({ key: "q", label: `搜索：${q}` })
    if (applied.mallId !== "all") {
        const mallName = malls.find((m) => m.id === applied.mallId)?.name
        chips.push({
            key: "mall",
            label: `来源商城：${mallName ?? applied.mallId}`,
        })
    }
    if (applied.attributionStatus !== "all") {
        chips.push({
            key: "attributionStatus",
            label: `归集：${ATTRIBUTION_STATUS_LABEL[applied.attributionStatus]}`,
        })
    }
    if (applied.fulfillmentChain !== "all") {
        chips.push({
            key: "fulfillmentChain",
            label: `履约链：${FULFILLMENT_CHAIN_LABEL[applied.fulfillmentChain]}`,
        })
    }
    if (applied.paymentSource !== "all") {
        chips.push({
            key: "paymentSource",
            label: `支付方式：${PAYMENT_SOURCE_LABEL[applied.paymentSource]}`,
        })
    }
    if (applied.costBasis !== "all") {
        chips.push({
            key: "costBasis",
            label: `成本口径：${COST_BASIS_LABEL[applied.costBasis]}`,
        })
    }
    if (applied.factTypes.length > 0) {
        chips.push({
            key: "factTypes",
            label: `事实类型：${applied.factTypes
                .map((factType) => FACT_TYPE_LABEL[factType])
                .join("、")}`,
        })
    }
    if (applied.supplierStatuses.length > 0) {
        chips.push({
            key: "supplierStatuses",
            label: `供应商状态：${applied.supplierStatuses
                .map((status) => SUPPLIER_STATUS_LABEL[status])
                .join("、")}`,
        })
    }
    if (applied.dataSources.length > 0) {
        chips.push({
            key: "dataSources",
            label: `数据来源：${applied.dataSources
                .map((source) => DATA_SOURCE_LABEL[source])
                .join("、")}`,
        })
    }
    if (applied.metric !== "all") {
        chips.push({
            key: "metric",
            label: `指标：${MALL_CONSUMPTION_METRIC_LABELS[applied.metric]}`,
        })
    }
    return chips
}

/** 面板固定单选选项。 */
export const ATTRIBUTION_STATUS_OPTIONS: ReadonlyArray<{
    value: AttributionStatus | "all"
    label: string
}> = [
    { value: "all", label: "全部" },
    { value: "ATTRIBUTED", label: ATTRIBUTION_STATUS_LABEL.ATTRIBUTED },
    { value: "PENDING", label: ATTRIBUTION_STATUS_LABEL.PENDING },
    { value: "DIFFERENCE", label: ATTRIBUTION_STATUS_LABEL.DIFFERENCE },
]

export const FULFILLMENT_CHAIN_OPTIONS: ReadonlyArray<{
    value: FulfillmentChain | "all"
    label: string
}> = [
    { value: "all", label: "全部" },
    {
        value: "LEGACY_MANUAL",
        label: FULFILLMENT_CHAIN_LABEL.LEGACY_MANUAL,
    },
    { value: "ERP_AUTOMATED", label: FULFILLMENT_CHAIN_LABEL.ERP_AUTOMATED },
]

export const PAYMENT_SOURCE_OPTIONS: ReadonlyArray<{
    value: PaymentSourceFilter | "all"
    label: string
}> = [
    { value: "all", label: "全部" },
    { value: "CARD", label: PAYMENT_SOURCE_LABEL.CARD },
    { value: "WECHAT", label: PAYMENT_SOURCE_LABEL.WECHAT },
    { value: "MIXED", label: PAYMENT_SOURCE_LABEL.MIXED },
]

export const COST_BASIS_OPTIONS: ReadonlyArray<{
    value: CostBasis | "all"
    label: string
}> = [
    { value: "all", label: "全部" },
    { value: "ACTUAL", label: COST_BASIS_LABEL.ACTUAL },
    { value: "STANDARD", label: COST_BASIS_LABEL.STANDARD },
    { value: "NONE", label: COST_BASIS_LABEL.NONE },
]

export const FACT_TYPE_OPTIONS: ReadonlyArray<{
    value: FactType
    label: string
}> = FACT_TYPES.map((factType) => ({
    value: factType,
    label: FACT_TYPE_LABEL[factType],
}))

export const DATA_SOURCE_OPTIONS: ReadonlyArray<{
    value: Exclude<DataSource, "MIXED">
    label: string
}> = DATA_SOURCES.map((source) => ({
    value: source,
    label: DATA_SOURCE_LABEL[source],
}))

export const SUPPLIER_STATUS_OPTIONS: ReadonlyArray<{
    value: SupplierFulfillmentStatus
    label: string
}> = SUPPLIER_STATUSES.map((status) => ({
    value: status,
    label: SUPPLIER_STATUS_LABEL[status],
}))
