import * as React from "react"

import {
    EMPTY_SALES_ORDERS_LIST_FILTER_DRAFT,
    filterDraftFromUrl,
    hasStructuredSalesOrdersFilters,
    resolveSalesOrdersListFilterPatch,
    salesOrdersListFilterDraftsEqual,
} from "@/features/sales-orders/lib/sales-orders-list-filters"
import type { SalesOrdersUrlState } from "@/features/sales-orders/lib/url-state"

/** 可被单独移除的已生效条件。 */
export type SalesOrdersListFilterKey =
    | "search"
    | "summary"
    | "customerId"
    | "contractId"
    | "createdBy"
    | "ownerUserIds"
    | "orgUnitIds"
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
    const [filterPanelOpen, setFilterPanelOpen] = React.useState(false)
    const [filterDraft, setFilterDraft] = React.useState(() =>
        filterDraftFromUrl(url),
    )

    const hasStructuredFilters = hasStructuredSalesOrdersFilters(url)

    const {
        search,
        customerId,
        contractId,
        createdBy,
        ownerUserIds,
        orgUnitIds,
        includeDescendants,
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

    // URL 回填只同步草稿；深链条件显示为已生效标签，不自动打开浮层。
    React.useEffect(() => {
        setSearchDraft(search ?? "")
        setFilterDraft(
            filterDraftFromUrl({
                customerId,
                contractId,
                createdBy,
                ownerUserIds,
                orgUnitIds,
                includeDescendants,
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
        ownerUserIds,
        orgUnitIds,
        includeDescendants,
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
                case "ownerUserIds":
                    pushUrl({ ownerUserIds: undefined, page: 1 })
                    break
                case "orgUnitIds":
                    pushUrl({
                        orgUnitIds: undefined,
                        includeDescendants: false,
                        page: 1,
                    })
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

    /** 仅清除面板草稿；保留常驻条件与当前结果。 */
    const resetMoreFilters = React.useCallback(() => {
        setFilterDraft((draft) => ({
            ...EMPTY_SALES_ORDERS_LIST_FILTER_DRAFT,
            nature: draft.nature,
            ownerUserIds: draft.ownerUserIds,
            commercialStatus: draft.commercialStatus,
            createdFrom: draft.createdFrom,
            createdTo: draft.createdTo,
        }))
    }, [])

    /** 取消、外点和 Esc 均撤销未应用的面板条件，保留外部查询栏草稿。 */
    const cancelMoreFilters = React.useCallback(() => {
        setFilterDraft((draft) => ({
            ...filterDraftFromUrl(url),
            nature: draft.nature,
            ownerUserIds: draft.ownerUserIds,
            commercialStatus: draft.commercialStatus,
            createdFrom: draft.createdFrom,
            createdTo: draft.createdTo,
        }))
        setFilterPanelOpen(false)
    }, [url])

    const hasPendingChanges =
        searchDraft.trim() !== (search ?? "").trim() ||
        !salesOrdersListFilterDraftsEqual(filterDraft, filterDraftFromUrl(url))

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
            ownerUserIds: undefined,
            orgUnitIds: undefined,
            includeDescendants: false,
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
        hasPendingChanges,
        applyFilters,
        removeFilter,
        resetMoreFilters,
        cancelMoreFilters,
        clearFilters,
    }
}
