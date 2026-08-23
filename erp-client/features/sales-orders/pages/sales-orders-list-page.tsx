"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import type { PaginationState, SortingState } from "@tanstack/react-table"

import { BusinessTableFrame, PageScaffold } from "@/components/business"
import { toast } from "@/components/ui/toast"
import { getErrorMessage } from "@/lib/api/errors"
import { downloadSalesOrderContractPdf } from "@/features/sales-orders/api/sales-orders"
import { SalesOrdersListFilterBar } from "@/features/sales-orders/components/sales-orders-list-filter-bar"
import { SalesOrdersListFilterPanel } from "@/features/sales-orders/components/sales-orders-list-filter-panel"
import { SalesOrdersListHeader } from "@/features/sales-orders/components/sales-orders-list-header"
import { SalesOrdersListTable } from "@/features/sales-orders/components/sales-orders-list-table"
import { useSalesOrdersListChips } from "@/features/sales-orders/hooks/use-sales-orders-list-chips"
import { useSalesOrdersListExport } from "@/features/sales-orders/hooks/use-sales-orders-list-export"
import { useSalesOrdersListFilters } from "@/features/sales-orders/hooks/use-sales-orders-list-filters"
import { useSalesOrdersListQuery } from "@/features/sales-orders/hooks/use-sales-orders-list-query"
import { useSalesOrdersListUrlState } from "@/features/sales-orders/hooks/use-sales-orders-list-url-state"
import {
    salesOrdersListFilterDescription,
    salesOrdersListFiltersActive,
} from "@/features/sales-orders/lib/sales-orders-list-filters"
import { SORT_COLUMN_TO_FIELD } from "@/features/sales-orders/lib/sales-orders-list-query"
import type { SalesOrderListItem } from "@/features/sales-orders/types"

/**
 * 销售单列表。实物/卡券销售单走各自审批入口。
 * SalesReturnCase 为 NO_APPROVAL，列表不展示销售退货审批区或审批动作。
 */
export function SalesOrdersListPage() {
    const router = useRouter()
    const { url, pushUrl } = useSalesOrdersListUrlState()
    const { ordersQuery, query } = useSalesOrdersListQuery(url)
    const {
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
    } = useSalesOrdersListFilters(url, pushUrl)

    const panelId = React.useId()

    const items = React.useMemo(
        () => ordersQuery.data?.items ?? [],
        [ordersQuery.data?.items],
    )
    const total = ordersQuery.data?.total ?? 0

    const { exportJob, exportCsv, isExporting } = useSalesOrdersListExport(
        query,
        total,
    )
    const chips = useSalesOrdersListChips(url, items, removeFilter)

    const [downloadingContractId, setDownloadingContractId] = React.useState<
        string | null
    >(null)

    const handleRowNavigate = React.useCallback(
        (id: string) => {
            router.push(`/sales/orders/${id}`)
        },
        [router],
    )

    const downloadContract = React.useCallback(
        async (order: SalesOrderListItem) => {
            const contractId = order.contractId.trim()
            if (!contractId || downloadingContractId) return
            setDownloadingContractId(contractId)
            try {
                await downloadSalesOrderContractPdf(contractId)
            } catch (error) {
                toast.add({
                    title: "合同下载失败",
                    description: getErrorMessage(error, "请稍后重试"),
                    type: "error",
                    timeout: 4000,
                })
            } finally {
                setDownloadingContractId(null)
            }
        },
        [downloadingContractId],
    )

    const filtersActive = salesOrdersListFiltersActive(url)

    const pagination = React.useMemo<PaginationState>(
        () => ({
            pageIndex: Math.max(0, url.page - 1),
            pageSize: url.pageSize,
        }),
        [url.page, url.pageSize],
    )

    const handlePaginationChange = React.useCallback(
        (next: PaginationState) => {
            pushUrl({ page: next.pageIndex + 1, pageSize: next.pageSize })
        },
        [pushUrl],
    )

    const sorting = React.useMemo<SortingState>(
        () =>
            url.sort && SORT_COLUMN_TO_FIELD[url.sort]
                ? [{ id: url.sort, desc: url.dir === "desc" }]
                : [],
        [url.dir, url.sort],
    )

    const handleSortingChange = React.useCallback(
        (next: SortingState) => {
            const head = next[0]
            pushUrl({
                sort:
                    head && SORT_COLUMN_TO_FIELD[head.id] ? head.id : undefined,
                dir: head ? (head.desc ? "desc" : "asc") : undefined,
                page: 1,
            })
        },
        [pushUrl],
    )

    return (
        <PageScaffold density="compact">
            <SalesOrdersListHeader
                isError={ordersQuery.isError}
                isFetching={ordersQuery.isFetching}
                queriedAt={ordersQuery.data?.queriedAt}
                exportDisabled={total === 0 || isExporting}
                onExport={() => {
                    void exportCsv()
                }}
                exportJob={exportJob}
            />

            <BusinessTableFrame
                showHeader
                title={
                    <span className="inline-flex items-baseline gap-2">
                        销售单
                        <span
                            className="font-normal text-muted-foreground"
                            aria-live="polite"
                        >
                            {total} 条
                        </span>
                    </span>
                }
                description={salesOrdersListFilterDescription(url)}
                toolbar={
                    <SalesOrdersListFilterBar
                        panelId={panelId}
                        searchDraft={searchDraft}
                        onSearchDraftChange={setSearchDraft}
                        onSubmit={applyFilters}
                        filterPanelOpen={filterPanelOpen}
                        onToggleFilterPanel={() => {
                            setFilterPanelOpen((open) => !open)
                        }}
                        hasStructuredFilters={hasStructuredFilters}
                        hasChips={filtersActive && chips.length > 0}
                        chips={chips}
                        onClearFilters={clearFilters}
                        summary={url.summary}
                        onSummaryChange={(summary) => {
                            // 工作视图会约束创建人或审核轨；切换时清掉重叠条件，避免同字段冲突。
                            pushUrl({
                                summary,
                                createdBy: undefined,
                                commercialStatus: "all",
                                reviewStatus: "all",
                                page: 1,
                            })
                        }}
                        filterPanel={
                            <SalesOrdersListFilterPanel
                                panelId={panelId}
                                draft={filterDraft}
                                onDraftChange={setFilterDraft}
                                onResetMoreFilters={resetMoreFilters}
                            />
                        }
                    />
                }
                table={
                    <SalesOrdersListTable
                        items={items}
                        total={total}
                        loading={ordersQuery.isPending}
                        isError={ordersQuery.isError}
                        error={ordersQuery.error}
                        onRetry={() => {
                            void ordersQuery.refetch()
                        }}
                        filtersActive={filtersActive}
                        onClearFilters={clearFilters}
                        url={url}
                        sorting={sorting}
                        onSortingChange={handleSortingChange}
                        pagination={pagination}
                        onPaginationChange={handlePaginationChange}
                        onRowNavigate={handleRowNavigate}
                        downloadingContractId={downloadingContractId}
                        downloadContract={downloadContract}
                    />
                }
            />
        </PageScaffold>
    )
}
