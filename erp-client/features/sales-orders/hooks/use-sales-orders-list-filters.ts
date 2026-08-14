import * as React from "react"

import {
    EMPTY_SALES_ORDERS_LIST_FILTER_DRAFT,
    filterDraftFromUrl,
    hasStructuredSalesOrdersFilters,
    resolveSalesOrdersListFilterPatch,
} from "@/features/sales-orders/lib/sales-orders-list-filters"
import type { SalesOrdersUrlState } from "@/features/sales-orders/lib/url-state"

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
    const { summary } = url

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
        setFilterPanelOpen(hasStructuredFilters)
    }, [
        hasStructuredFilters,
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
                summary,
                searchDraft,
                filterDraft,
            }),
        )
    }, [filterDraft, pushUrl, searchDraft, summary])

    const clearFilters = React.useCallback(() => {
        setSearchDraft("")
        setFilterPanelOpen(false)
        setFilterDraft(EMPTY_SALES_ORDERS_LIST_FILTER_DRAFT)
        pushUrl({
            search: undefined,
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
        clearFilters,
    }
}
