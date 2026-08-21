import {
    salesOrderCloseLabel,
    salesOrderCollectionLabel,
    salesOrderCommercialStatusLabel,
    salesOrderFulfillmentLabel,
    salesOrderInvoiceLabel,
    salesOrderReviewStatusLabel,
    salesOrderSummaryLabels,
    type SalesOrderCloseFilter,
    type SalesOrderCollectionFilter,
    type SalesOrderCommercialStatusFilter,
    type SalesOrderFulfillmentFilter,
    type SalesOrderInvoiceFilter,
    type SalesOrderNatureFilter,
    type SalesOrderOriginFilter,
    type SalesOrderReviewStatusFilter,
    type SalesOrderSummaryFilter,
} from "@/features/sales-orders/lib/filter-orders"
import { NATURE_LABEL, ORIGIN_LABEL } from "@/features/sales-orders/lib/labels"
import type { SalesOrdersUrlState } from "@/features/sales-orders/lib/url-state"

export type SalesOrdersListFilterDraft = {
    customerId: string
    contractId: string
    createdBy: string
    nature: SalesOrderNatureFilter
    origin: SalesOrderOriginFilter
    commercialStatus: SalesOrderCommercialStatusFilter
    reviewStatus: SalesOrderReviewStatusFilter
    fulfillment: SalesOrderFulfillmentFilter
    collection: SalesOrderCollectionFilter
    invoice: SalesOrderInvoiceFilter
    closeStatus: SalesOrderCloseFilter
    createdFrom: string
    createdTo: string
}

export function hasStructuredSalesOrdersFilters(
    url: SalesOrdersUrlState,
): boolean {
    return Boolean(
        url.customerId ||
        url.contractId ||
        url.createdBy ||
        url.nature !== "all" ||
        url.origin !== "all" ||
        url.commercialStatus !== "all" ||
        url.reviewStatus !== "all" ||
        url.fulfillment !== "all" ||
        url.collection !== "all" ||
        url.invoice !== "all" ||
        url.closeStatus !== "all" ||
        url.createdFrom ||
        url.createdTo,
    )
}

export function salesOrdersListFiltersActive(
    url: SalesOrdersUrlState,
): boolean {
    return (
        Boolean(url.search) ||
        url.summary !== "all" ||
        hasStructuredSalesOrdersFilters(url)
    )
}

/** filterDraftFromUrl 所需的 URL 状态子集（无分页/排序等无关字段）。 */
export type SalesOrdersListFilterUrl = Pick<
    SalesOrdersUrlState,
    | "customerId"
    | "contractId"
    | "createdBy"
    | "nature"
    | "origin"
    | "commercialStatus"
    | "reviewStatus"
    | "fulfillment"
    | "collection"
    | "invoice"
    | "closeStatus"
    | "createdFrom"
    | "createdTo"
>

export function filterDraftFromUrl(
    url: SalesOrdersListFilterUrl,
): SalesOrdersListFilterDraft {
    return {
        customerId: url.customerId ?? "",
        contractId: url.contractId ?? "",
        createdBy: url.createdBy ?? "",
        nature: url.nature,
        origin: url.origin,
        commercialStatus: url.commercialStatus,
        reviewStatus: url.reviewStatus,
        fulfillment: url.fulfillment,
        collection: url.collection,
        invoice: url.invoice,
        closeStatus: url.closeStatus,
        createdFrom: url.createdFrom ?? "",
        createdTo: url.createdTo ?? "",
    }
}

export const EMPTY_SALES_ORDERS_LIST_FILTER_DRAFT: SalesOrdersListFilterDraft =
    {
        customerId: "",
        contractId: "",
        createdBy: "",
        nature: "all",
        origin: "all",
        commercialStatus: "all",
        reviewStatus: "all",
        fulfillment: "all",
        collection: "all",
        invoice: "all",
        closeStatus: "all",
        createdFrom: "",
        createdTo: "",
    }

/**
 * 草稿落定到 URL 的补丁：交换反向日期区间、清空空白搜索词，并在草稿与固定
 * 工作视图同字段冲突时回退为「全部」视图（与列表页原始行为一致）。
 */
export function resolveSalesOrdersListFilterPatch(input: {
    summary: SalesOrderSummaryFilter
    searchDraft: string
    filterDraft: SalesOrdersListFilterDraft
}): Partial<SalesOrdersUrlState> {
    const { summary, searchDraft, filterDraft } = input
    const [createdFrom, createdTo] =
        filterDraft.createdFrom &&
        filterDraft.createdTo &&
        filterDraft.createdFrom > filterDraft.createdTo
            ? [filterDraft.createdTo, filterDraft.createdFrom]
            : [filterDraft.createdFrom, filterDraft.createdTo]
    const summaryConflictsWithDraft =
        (summary === "mine" &&
            (Boolean(filterDraft.createdBy) ||
                filterDraft.commercialStatus !== "all" ||
                filterDraft.reviewStatus !== "all")) ||
        (summary === "createdByMe" && Boolean(filterDraft.createdBy)) ||
        (summary === "exception" &&
            (filterDraft.commercialStatus !== "all" ||
                filterDraft.reviewStatus !== "all"))

    return {
        search: searchDraft.trim() || undefined,
        customerId: filterDraft.customerId || undefined,
        contractId: filterDraft.contractId || undefined,
        createdBy: filterDraft.createdBy || undefined,
        nature: filterDraft.nature,
        summary: summaryConflictsWithDraft ? "all" : summary,
        origin: filterDraft.origin,
        commercialStatus: filterDraft.commercialStatus,
        reviewStatus: filterDraft.reviewStatus,
        fulfillment: filterDraft.fulfillment,
        collection: filterDraft.collection,
        invoice: filterDraft.invoice,
        closeStatus: filterDraft.closeStatus,
        createdFrom: createdFrom || undefined,
        createdTo: createdTo || undefined,
        page: 1,
    }
}

export function salesOrdersListFilterDescription(
    url: SalesOrdersUrlState,
): string {
    if (!salesOrdersListFiltersActive(url)) {
        return "设置一个或多个条件后统一搜索；筛选条件会保存在网址中，便于刷新、返回与分享。"
    }
    return `当前筛选：${[
        url.summary !== "all" ? salesOrderSummaryLabels(url.summary) : null,
        url.nature !== "all" ? NATURE_LABEL[url.nature] : null,
        url.origin !== "all" ? ORIGIN_LABEL[url.origin] : null,
        url.commercialStatus !== "all"
            ? salesOrderCommercialStatusLabel(url.commercialStatus)
            : null,
        url.reviewStatus !== "all"
            ? salesOrderReviewStatusLabel(url.reviewStatus)
            : null,
        url.fulfillment !== "all"
            ? salesOrderFulfillmentLabel(url.fulfillment)
            : null,
        url.collection !== "all"
            ? salesOrderCollectionLabel(url.collection)
            : null,
        url.invoice !== "all" ? salesOrderInvoiceLabel(url.invoice) : null,
        url.closeStatus !== "all"
            ? salesOrderCloseLabel(url.closeStatus)
            : null,
        url.customerId ? "已选客户" : null,
        url.contractId ? "已选合同" : null,
        url.createdBy ? "已选创建人" : null,
        url.createdFrom || url.createdTo
            ? `创建日期 ${url.createdFrom || "不限"} 至 ${url.createdTo || "不限"}`
            : null,
        url.search ? `关键词“${url.search}”` : null,
    ]
        .filter(Boolean)
        .join(" · ")}`
}
