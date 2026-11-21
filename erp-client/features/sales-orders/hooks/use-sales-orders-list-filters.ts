import * as React from "react"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"

import { useSearchDraft } from "@/lib/use-search-draft"

import {
    EMPTY_SALES_ORDERS_LIST_FILTER_DRAFT,
    type SalesOrdersListFilterDraft,
    filterDraftFromUrl,
    resolveSalesOrdersListFilterPatch,
    salesOrdersListFilterDraftsEqual,
} from "@/features/sales-orders/lib/sales-orders-list-filters"
import type { SalesOrderSummaryFilter } from "@/features/sales-orders/lib/filter-orders"
import type { SalesOrdersUrlState } from "@/features/sales-orders/lib/url-state"

/** 可被单独移除的已生效条件。 */
export type SalesOrdersListFilterKey =
    | "search"
    | "summary"
    | "customerId"
    | "contractId"
    | "hasContract"
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
    const { searchDraft, setSearchDraft, searchInputRef } = useSearchDraft(
        url.search,
    )
    const [filterPanelOpen, setFilterPanelOpen] = React.useState(false)
    const [draft, setFilterDraft] = React.useState(() =>
        filterDraftFromUrl(url),
    )

    const account = useAccountProfileQuery()
    const permissions = account.data?.permissions
    const stripUnavailable = React.useCallback(
        (value: SalesOrdersListFilterDraft) => ({
            ...value,
            customerId: hasPermission(permissions, "customer:list")
                ? value.customerId
                : "",
            contractId: hasPermission(permissions, "contract:list")
                ? value.contractId
                : "",
            createdBy: hasPermission(permissions, "business_person:list")
                ? value.createdBy
                : "",
            ownerUserIds: hasPermission(permissions, "sales_person:list")
                ? value.ownerUserIds
                : "",
            orgUnitIds: hasPermission(permissions, "org_unit:list")
                ? value.orgUnitIds
                : "",
            includeDescendants:
                hasPermission(permissions, "org_unit:list") &&
                value.includeDescendants,
        }),
        [permissions],
    )
    const filterDraft = React.useMemo(
        () => stripUnavailable(draft),
        [draft, stripUnavailable],
    )
    const {
        search,
        customerId,
        contractId,
        hasContract,
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

    // 分页与排序不应覆盖筛选草稿。
    const appliedDraft = React.useMemo(
        () =>
            filterDraftFromUrl({
                customerId,
                contractId,
                hasContract,
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
        [
            closeStatus,
            collection,
            commercialStatus,
            contractId,
            hasContract,
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
        ],
    )
    const previousAppliedDraft = React.useRef(appliedDraft)
    React.useEffect(() => {
        const previous = previousAppliedDraft.current
        previousAppliedDraft.current = appliedDraft
        setFilterDraft((draft) => ({
            ...appliedDraft,
            ownerUserIds:
                previous.ownerUserIds === appliedDraft.ownerUserIds
                    ? draft.ownerUserIds
                    : appliedDraft.ownerUserIds,
            commercialStatus:
                previous.commercialStatus === appliedDraft.commercialStatus
                    ? draft.commercialStatus
                    : appliedDraft.commercialStatus,
        }))
    }, [appliedDraft])

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
                case "hasContract":
                    pushUrl({ hasContract: "all", page: 1 })
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
            ownerUserIds: draft.ownerUserIds,
            commercialStatus: draft.commercialStatus,
        }))
    }, [])

    /** 收起面板时仅撤销更多条件的草稿。 */
    const cancelMoreFilters = React.useCallback(() => {
        setFilterDraft((draft) => ({
            ...appliedDraft,
            ownerUserIds: draft.ownerUserIds,
            commercialStatus: draft.commercialStatus,
        }))
        setFilterPanelOpen(false)
    }, [appliedDraft])

    const hasPendingChanges =
        searchDraft.trim() !== (search ?? "").trim() ||
        !salesOrdersListFilterDraftsEqual(filterDraft, appliedDraft)

    const changeSummary = React.useCallback(
        (summary: SalesOrderSummaryFilter) => {
            setFilterPanelOpen(false)
            setFilterDraft((draft) => ({
                ...appliedDraft,
                ownerUserIds: draft.ownerUserIds,
                createdBy: "",
                commercialStatus: "all",
                reviewStatus: "all",
            }))
            pushUrl({
                summary,
                createdBy: undefined,
                commercialStatus: "all",
                reviewStatus: "all",
                page: 1,
            })
        },
        [appliedDraft, pushUrl],
    )

    const clearFilters = React.useCallback(() => {
        setSearchDraft("")
        setFilterPanelOpen(false)
        setFilterDraft(EMPTY_SALES_ORDERS_LIST_FILTER_DRAFT)
        pushUrl({
            search: undefined,
            summary: "all",
            customerId: undefined,
            contractId: undefined,
            hasContract: "all",
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
    }, [pushUrl, setSearchDraft])

    return {
        searchDraft,
        setSearchDraft,
        searchInputRef,
        filterDraft,
        setFilterDraft,
        filterPanelOpen,
        setFilterPanelOpen,
        hasPendingChanges,
        applyFilters,
        removeFilter,
        resetMoreFilters,
        cancelMoreFilters,
        clearFilters,
        changeSummary,
    }
}
