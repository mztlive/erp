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
