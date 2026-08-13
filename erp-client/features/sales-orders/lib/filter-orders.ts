import type { SalesOrderListItem } from "@/features/sales-orders/types"

export type SalesOrderNatureFilter = "all" | "physical_service" | "card_voucher"

/**
 * 列表四个固定工作视图：全部由后端筛选参数实现（`created_by`/`my_todo`/
 * `exception_only`），不是前端对已拉取行的二次过滤——一旦筛选与分页脱节，
 * 页码和"共 N 条"就会对不上。
 */
export type SalesOrderSummaryFilter =
    | "all"
    | "mine"
    | "createdByMe"
    | "exception"

export type SalesOrderOriginFilter = "all" | SalesOrderListItem["originSystem"]

export type SalesOrderCommercialStatusFilter =
    | "all"
    | "draft"
    | "pending_review"
    | "effective"
    | "voided"

export type SalesOrderReviewStatusFilter =
    | "all"
    | "not_submitted"
    | "pending_procurement_confirmation"
    | "pending_low_margin_superior"
    | "pending_sales_leader"
    | "pending_operations"
    | "approved"
    | "rejected"

export type SalesOrderFulfillmentFilter =
    | "all"
    | "not_started"
    | "partially_fulfilled"
    | "completed"

export type SalesOrderCollectionFilter =
    | "all"
    | "not_collected"
    | "partially_collected"
    | "settled"

export type SalesOrderInvoiceFilter =
    | "all"
    | "not_invoiced"
    | "partially_invoiced"
    | "completed"

export type SalesOrderCloseFilter =
    | "all"
    | "not_satisfied"
    | "closeable"
    | "closed"

export const SALES_ORDER_COMMERCIAL_STATUS_OPTIONS = [
    { value: "draft", label: "草稿" },
    { value: "pending_review", label: "审核中" },
    { value: "effective", label: "已生效" },
    { value: "voided", label: "已作废" },
] as const

export const SALES_ORDER_REVIEW_STATUS_OPTIONS = [
    { value: "not_submitted", label: "未提交" },
    { value: "pending_procurement_confirmation", label: "待采购确认" },
    { value: "pending_low_margin_superior", label: "待低毛利上级确认" },
    { value: "pending_sales_leader", label: "待销售领导" },
    { value: "pending_operations", label: "待运营" },
    { value: "approved", label: "已通过" },
    { value: "rejected", label: "已驳回" },
] as const

export const SALES_ORDER_FULFILLMENT_OPTIONS = [
    { value: "not_started", label: "未开始" },
    { value: "partially_fulfilled", label: "部分履约" },
    { value: "completed", label: "已完成" },
] as const

export const SALES_ORDER_COLLECTION_OPTIONS = [
    { value: "not_collected", label: "未收" },
    { value: "partially_collected", label: "部分回款" },
    { value: "settled", label: "已结清" },
] as const

export const SALES_ORDER_INVOICE_OPTIONS = [
    { value: "not_invoiced", label: "未开" },
    { value: "partially_invoiced", label: "部分开票" },
    { value: "completed", label: "已完成" },
] as const

export const SALES_ORDER_CLOSE_OPTIONS = [
    { value: "not_satisfied", label: "未满足关闭" },
    { value: "closeable", label: "可关闭" },
    { value: "closed", label: "已关闭" },
] as const

/** 主状态筛选：URL 使用稳定枚举码，中文映射集中在本文件，禁止三处重复维护。 */
export const SALES_ORDER_STATUS_OPTIONS = [
    { value: "awaiting_confirm", label: "待二次确认" },
    { value: "awaiting_sales", label: "待销售处理" },
    { value: "awaiting_sales_lead", label: "待销售领导审批" },
    { value: "awaiting_ops", label: "待运营审批" },
    { value: "fulfilling", label: "履约中" },
    { value: "effective", label: "已生效" },
    { value: "closed", label: "已关闭" },
    { value: "draft", label: "草稿" },
    { value: "voided", label: "已作废" },
] as const

export type SalesOrderStatusValue =
    (typeof SALES_ORDER_STATUS_OPTIONS)[number]["value"]

export type SalesOrderStatusFilter = "all" | SalesOrderStatusValue

const STATUS_LABEL_BY_VALUE = new Map<string, string>(
    SALES_ORDER_STATUS_OPTIONS.map((option) => [option.value, option.label]),
)

/** 主状态中文名（列表渲染 / 高级筛选文案共用）。 */
export function salesOrderStatusLabel(status: SalesOrderStatusFilter): string {
    if (status === "all") return "全部状态"
    return STATUS_LABEL_BY_VALUE.get(status) ?? status
}

function optionLabel(
    options: readonly { value: string; label: string }[],
    value: string,
    fallback: string,
): string {
    if (value === "all") return fallback
    return options.find((option) => option.value === value)?.label ?? value
}

export const salesOrderCommercialStatusLabel = (
    value: SalesOrderCommercialStatusFilter,
) => optionLabel(SALES_ORDER_COMMERCIAL_STATUS_OPTIONS, value, "全部商业状态")

export const salesOrderReviewStatusLabel = (
    value: SalesOrderReviewStatusFilter,
) => optionLabel(SALES_ORDER_REVIEW_STATUS_OPTIONS, value, "全部审核状态")

export const salesOrderFulfillmentLabel = (
    value: SalesOrderFulfillmentFilter,
) => optionLabel(SALES_ORDER_FULFILLMENT_OPTIONS, value, "全部履约进度")

export const salesOrderCollectionLabel = (value: SalesOrderCollectionFilter) =>
    optionLabel(SALES_ORDER_COLLECTION_OPTIONS, value, "全部回款进度")

export const salesOrderInvoiceLabel = (value: SalesOrderInvoiceFilter) =>
    optionLabel(SALES_ORDER_INVOICE_OPTIONS, value, "全部开票进度")

export const salesOrderCloseLabel = (value: SalesOrderCloseFilter) =>
    optionLabel(SALES_ORDER_CLOSE_OPTIONS, value, "全部关闭状态")

export function salesOrderSummaryLabels(
    summaryFilter: SalesOrderSummaryFilter,
): string {
    switch (summaryFilter) {
        case "mine":
            return "待我处理"
        case "createdByMe":
            return "我创建的"
        case "exception":
            return "异常"
        default:
            return "全部"
    }
}
