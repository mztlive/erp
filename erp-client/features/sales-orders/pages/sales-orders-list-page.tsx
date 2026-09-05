"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import type { PaginationState, SortingState } from "@tanstack/react-table"

import { PageScaffold } from "@/components/business"
import {
    ListWorkSurface,
    ListWorkspaceViews,
    listWorkspaceStyles as styles,
} from "@/components/business/list-workspace"
import { salesOrdersListStyles as salesStyles } from "./sales-orders-list-styles"
import { toast } from "@/components/ui/toast"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { downloadSalesOrderContractPdf } from "@/features/sales-orders/api/sales-orders"
import { SalesOrdersListFilterBar } from "@/features/sales-orders/components/sales-orders-list-filter-bar"
import { SalesOrdersListHeader } from "@/features/sales-orders/components/sales-orders-list-header"
import { SalesOrdersListTable } from "@/features/sales-orders/components/sales-orders-list-table"
import type { SalesOrderSummaryFilter } from "@/features/sales-orders/lib/filter-orders"
import { useSalesOrdersListChips } from "@/features/sales-orders/hooks/use-sales-orders-list-chips"
import { useSalesOrdersListExport } from "@/features/sales-orders/hooks/use-sales-orders-list-export"
import { useSalesOrdersListFilters } from "@/features/sales-orders/hooks/use-sales-orders-list-filters"
import { useSalesOrdersListQuery } from "@/features/sales-orders/hooks/use-sales-orders-list-query"
import { useSalesOrdersListUrlState } from "@/features/sales-orders/hooks/use-sales-orders-list-url-state"
import { salesOrdersListFiltersActive } from "@/features/sales-orders/lib/sales-orders-list-filters"
import { SORT_COLUMN_TO_FIELD } from "@/features/sales-orders/lib/sales-orders-list-query"
import type { SalesOrderListItem } from "@/features/sales-orders/types"

const SUMMARY_FILTER_OPTIONS: ReadonlyArray<{
    value: SalesOrderSummaryFilter
    label: string
}> = [
    { value: "all", label: "全部" },
    { value: "mine", label: "待我处理" },
    { value: "createdByMe", label: "我创建的" },
    { value: "exception", label: "异常" },
]

/**
 * 销售单列表。实物/卡券销售单走各自审批入口。
 * SalesReturnCase 为 NO_APPROVAL，列表不展示销售退货审批区或审批动作。
 */
export function SalesOrdersListPage() {
    const router = useRouter()
    const { url, pushUrl } = useSalesOrdersListUrlState()
    const { ordersQuery, query } = useSalesOrdersListQuery(url)
    const filters = useSalesOrdersListFilters(url, pushUrl)
    const { removeFilter, clearFilters } = filters

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
        <PageScaffold density="compact" className={styles.page}>
            <SalesOrdersListHeader
                isError={ordersQuery.isError}
                isFetching={ordersQuery.isFetching}
                queriedAt={ordersQuery.data?.queriedAt}
                exportDisabled={total === 0 || isExporting}
                isExporting={isExporting}
                onExport={() => {
                    void exportCsv()
                }}
                exportJob={exportJob}
            />

            <ListWorkSurface
                ariaLabel="销售单列表"
                views={
                    <ListWorkspaceViews
                        ariaLabel="销售单工作视图"
                        hint="选择销售单查看详情"
                        items={SUMMARY_FILTER_OPTIONS.map((option) => {
                            const active = url.summary === option.value
                            return {
                                id: `sales-orders-list-filter-summary-${toAutomationIdSegment(option.value)}`,
                                label: option.label,
                                count: active ? total : undefined,
                                active,
                                onClick: () => {
                                    // 工作视图会约束创建人或审核轨；切换时清掉重叠条件，避免同字段冲突。
                                    pushUrl({
                                        summary: option.value,
                                        createdBy: undefined,
                                        commercialStatus: "all",
                                        reviewStatus: "all",
                                        page: 1,
                                    })
                                },
                            }
                        })}
                    />
                }
                toolbar={
                    <SalesOrdersListFilterBar
                        filters={filters}
                        chips={chips}
                        resultCount={ordersQuery.data ? total : undefined}
                        loading={ordersQuery.isFetching}
                        failed={ordersQuery.isError}
                    />
                }
                tableClassName={salesStyles.table}
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
