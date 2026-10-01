import {
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
import type { SalesOrdersUrlState } from "@/features/sales-orders/lib/url-state"

export type SalesOrdersListFilterDraft = {
    customerId: string
    contractId: string
    ownerUserIds: string
    orgUnitIds: string
    includeDescendants: boolean
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

export function hasMoreSalesOrdersFilters(url: SalesOrdersUrlState): boolean {
    return Boolean(
        url.customerId ||
        url.contractId ||
        url.createdBy ||
        url.ownerUserIds ||
        url.orgUnitIds ||
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

export function hasStructuredSalesOrdersFilters(
    url: SalesOrdersUrlState,
): boolean {
    return url.nature !== "all" || hasMoreSalesOrdersFilters(url)
}

export function salesOrdersListFilterDraftsEqual(
    left: SalesOrdersListFilterDraft,
    right: SalesOrdersListFilterDraft,
): boolean {
    return (
        left.customerId === right.customerId &&
        left.contractId === right.contractId &&
        left.createdBy === right.createdBy &&
        left.ownerUserIds === right.ownerUserIds &&
        left.orgUnitIds === right.orgUnitIds &&
        left.includeDescendants === right.includeDescendants &&
        left.nature === right.nature &&
        left.origin === right.origin &&
        left.commercialStatus === right.commercialStatus &&
        left.reviewStatus === right.reviewStatus &&
        left.fulfillment === right.fulfillment &&
        left.collection === right.collection &&
        left.invoice === right.invoice &&
        left.closeStatus === right.closeStatus &&
        left.createdFrom === right.createdFrom &&
        left.createdTo === right.createdTo
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
    | "ownerUserIds"
    | "orgUnitIds"
    | "includeDescendants"
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
        ownerUserIds: url.ownerUserIds ?? "",
        orgUnitIds: url.orgUnitIds ?? "",
        includeDescendants: url.includeDescendants,
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
        ownerUserIds: "",
        orgUnitIds: "",
        includeDescendants: false,
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
        ownerUserIds: filterDraft.ownerUserIds || undefined,
        orgUnitIds: filterDraft.orgUnitIds.trim() || undefined,
        includeDescendants: filterDraft.orgUnitIds.trim()
            ? filterDraft.includeDescendants
            : false,
        nature: filterDraft.nature,
        ...(summaryConflictsWithDraft ? { summary: "all" as const } : {}),
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
