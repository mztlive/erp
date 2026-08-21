import * as React from "react"

import {
    EMPTY_SALES_ORDERS_LIST_FILTER_DRAFT,
    filterDraftFromUrl,
    hasStructuredSalesOrdersFilters,
    resolveSalesOrdersListFilterPatch,
} from "@/features/sales-orders/lib/sales-orders-list-filters"
import type { SalesOrdersUrlState } from "@/features/sales-orders/lib/url-state"

/** 可被单独移除的已生效条件。 */
export type SalesOrdersListFilterKey =
    | "search"
    | "summary"
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
    | "createdDate"

/** 列表页筛选草稿：搜索词、高级筛选面板、URL 变更时的重同步与落定。 */
export function useSalesOrdersListFilters(
    url: SalesOrdersUrlState,
    pushUrl: (patch: Partial<SalesOrdersUrlState>) => void,
) {
    const [searchDraft, setSearchDraft] = React.useState(url.search ?? "")
    const [filterPanelOpen, setFilterPanelOpen] = React.useState(
        hasStructuredSalesOrdersFilters(url),
    )
    const [filterDraft, setFilterDraft] = React.useState(() =>
        filterDraftFromUrl(url),
    )

    const hasStructuredFilters = hasStructuredSalesOrdersFilters(url)

    const {
        search,
        customerId,
        contractId,
        createdBy,
        nature,
        origin,
        commercialStatus,
        reviewStatus,
        fulfillment,
        collection,
        invoice,
        closeStatus,
        createdFrom,
        createdTo,
    } = url

    // URL 回填只同步草稿；面板展开态由用户控制，初始深链展开见 useState 初值。
    React.useEffect(() => {
        setSearchDraft(search ?? "")
        setFilterDraft(
            filterDraftFromUrl({
                customerId,
                contractId,
                createdBy,
                nature,
                origin,
                commercialStatus,
                reviewStatus,
                fulfillment,
                collection,
                invoice,
                closeStatus,
                createdFrom,
                createdTo,
            }),
        )
    }, [
        closeStatus,
        collection,
        commercialStatus,
        contractId,
        createdBy,
        createdFrom,
        createdTo,
        customerId,
        fulfillment,
        invoice,
        nature,
        origin,
        reviewStatus,
        search,
    ])

    const applyFilters = React.useCallback(() => {
        pushUrl(
            resolveSalesOrdersListFilterPatch({
                summary: url.summary,
                searchDraft,
                filterDraft,
            }),
        )
        setFilterPanelOpen(false)
    }, [filterDraft, pushUrl, searchDraft, url.summary])

    /** 移除单个已生效条件；客户与合同一同移除（合同依赖客户）。 */
    const removeFilter = React.useCallback(
        (key: SalesOrdersListFilterKey) => {
            switch (key) {
                case "search":
                    pushUrl({ search: undefined, page: 1 })
                    break
                case "summary":
                    pushUrl({ summary: "all", page: 1 })
                    break
                case "customerId":
                    pushUrl({
                        customerId: undefined,
                        contractId: undefined,
                        page: 1,
                    })
                    break
                case "contractId":
                    pushUrl({ contractId: undefined, page: 1 })
                    break
                case "createdBy":
                    pushUrl({ createdBy: undefined, page: 1 })
                    break
                case "nature":
                    pushUrl({ nature: "all", page: 1 })
                    break
                case "origin":
                    pushUrl({ origin: "all", page: 1 })
                    break
                case "commercialStatus":
                    pushUrl({ commercialStatus: "all", page: 1 })
                    break
                case "reviewStatus":
                    pushUrl({ reviewStatus: "all", page: 1 })
                    break
                case "fulfillment":
                    pushUrl({ fulfillment: "all", page: 1 })
                    break
                case "collection":
                    pushUrl({ collection: "all", page: 1 })
                    break
                case "invoice":
                    pushUrl({ invoice: "all", page: 1 })
                    break
                case "closeStatus":
                    pushUrl({ closeStatus: "all", page: 1 })
                    break
                case "createdDate":
                    pushUrl({
                        createdFrom: undefined,
                        createdTo: undefined,
                        page: 1,
                    })
                    break
            }
        },
        [pushUrl],
    )

    /** 仅清除「更多筛选」；保留关键词与工作视图，保持面板展开。 */
    const resetMoreFilters = React.useCallback(() => {
        pushUrl({
            customerId: undefined,
            contractId: undefined,
            createdBy: undefined,
            nature: "all",
            origin: "all",
            commercialStatus: "all",
            reviewStatus: "all",
            fulfillment: "all",
            collection: "all",
            invoice: "all",
            closeStatus: "all",
            createdFrom: undefined,
            createdTo: undefined,
            page: 1,
        })
    }, [pushUrl])

    const clearFilters = React.useCallback(() => {
        setSearchDraft("")
        setFilterPanelOpen(false)
        setFilterDraft(EMPTY_SALES_ORDERS_LIST_FILTER_DRAFT)
        pushUrl({
            search: undefined,
            summary: "all",
            customerId: undefined,
            contractId: undefined,
            createdBy: undefined,
            nature: "all",
            origin: "all",
            commercialStatus: "all",
            reviewStatus: "all",
            fulfillment: "all",
            collection: "all",
            invoice: "all",
            closeStatus: "all",
            createdFrom: undefined,
            createdTo: undefined,
            page: 1,
        })
    }, [pushUrl])

    return {
        searchDraft,
        setSearchDraft,
        filterDraft,
        setFilterDraft,
        filterPanelOpen,
        setFilterPanelOpen,
        hasStructuredFilters,
        applyFilters,
        removeFilter,
        resetMoreFilters,
        clearFilters,
    }
}
