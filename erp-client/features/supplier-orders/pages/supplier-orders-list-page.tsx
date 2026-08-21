"use client"

import * as React from "react"
import type { PaginationState, SortingState } from "@tanstack/react-table"
import { DownloadIcon } from "lucide-react"

import {
    DataFreshness,
    FormalActionResult,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { SupplierOrdersListExportPreview } from "@/features/supplier-orders/components/supplier-orders-list-export"
import { SupplierOrdersListExportResult } from "@/features/supplier-orders/components/supplier-orders-list-export"
import { SupplierOrdersListMetricStrip } from "@/features/supplier-orders/components/supplier-orders-list-metric-strip"
import { SupplierOrdersListPreviewSheet } from "@/features/supplier-orders/components/supplier-orders-list-preview-sheet"
import { SupplierOrdersListReturnBanner } from "@/features/supplier-orders/components/supplier-orders-list-return-banner"
import { SupplierOrdersListTable } from "@/features/supplier-orders/components/supplier-orders-list-table"
import { SupplierOrdersListToolbar } from "@/features/supplier-orders/components/supplier-orders-list-toolbar"
import { useSupplierOrdersExport } from "@/features/supplier-orders/hooks/use-supplier-orders-export"
import { useSupplierOrdersFilters } from "@/features/supplier-orders/hooks/use-supplier-orders-filters"
import { useSupplierOrdersKeyboardNav } from "@/features/supplier-orders/hooks/use-supplier-orders-keyboard-nav"
import { useSupplierOrdersQueryResult } from "@/features/supplier-orders/hooks/use-supplier-orders-query-result"
import { useSupplierOrdersListColumns } from "@/features/supplier-orders/hooks/use-supplier-orders-list-columns"
import {
    useSupplierOrderDetailQuery,
    useSupplierOrdersQuery,
} from "@/features/supplier-orders/hooks/queries"
import type { SupplierOrderListQuery } from "@/features/supplier-orders/types"

const SORT_COLUMN_TO_FIELD: Record<
    string,
    NonNullable<SupplierOrderListQuery["sortBy"]>
> = {
    identity: "orderNo",
    mall: "mallOrderNo",
    external: "externalOrderNo",
    updated: "lastBusinessAt",
}

/** 结果卡默认操作说明（无筛选时展示）。 */
const DEFAULT_TABLE_DESCRIPTION =
    "身份列与操作列固定；履约/取消/退款三种状态独立展示。"

export function SupplierOrdersListPage() {
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const filters = useSupplierOrdersFilters(searchInputRef)
    const { url, returnTo, updateUrl } = filters

    const listQueryInput = React.useMemo<SupplierOrderListQuery>(
        () => ({
            view: url.view,
            q: url.q,
            supplierId: url.supplierId,
            fulfillmentStatuses: url.fulfillmentStatuses,
            cancelStatuses: url.cancelStatuses,
            refundStatuses: url.refundStatuses,
            aftersalePending: url.aftersalePending,
            paidFrom: url.paidFrom,
            paidTo: url.paidTo,
            page: url.page,
            pageSize: url.pageSize,
            sortBy: url.sort ? SORT_COLUMN_TO_FIELD[url.sort] : undefined,
            sortDir: url.dir,
        }),
        [url],
    )

    const listQuery = useSupplierOrdersQuery(listQueryInput)
    const previewQuery = useSupplierOrderDetailQuery({
        orderId: url.preview ?? "",
        enabled: Boolean(url.preview),
    })

    const rows = React.useMemo(
        () => listQuery.data?.rows ?? [],
        [listQuery.data?.rows],
    )
    const metrics = listQuery.data?.metrics ?? []
    const total = listQuery.data?.pageInfo.total ?? 0

    const {
        actionResult,
        dismissActionResult,
        queryPending,
        handleQueryFromList,
        queryFromPreview,
    } = useSupplierOrdersQueryResult({ updateUrl })

    const { focusedIndex, rowRefs } = useSupplierOrdersKeyboardNav({
        url,
        rows,
        updateUrl,
    })

    const {
        exportPreviewOpen,
        exportResult,
        pendingExport,
        exportMutation,
        openExportPreview,
        closeExportPreview,
        confirmExport,
        retryExport,
    } = useSupplierOrdersExport()

    const pagination = React.useMemo<PaginationState>(
        () => ({
            pageIndex: Math.max(0, url.page - 1),
            pageSize: url.pageSize,
        }),
        [url.page, url.pageSize],
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
            updateUrl({
                sort:
                    head && SORT_COLUMN_TO_FIELD[head.id] ? head.id : undefined,
                dir: head ? (head.desc ? "desc" : "asc") : undefined,
                page: 1,
            })
        },
        [updateUrl],
    )

    const openPreview = React.useCallback(
        (orderId: string) => updateUrl({ preview: orderId }, "push"),
        [updateUrl],
    )

    const closePreview = React.useCallback(() => {
        const id = url.preview
        updateUrl({ preview: undefined }, "push")
        if (id) {
            requestAnimationFrame(() => {
                rowRefs.current.get(id)?.focus()
            })
        }
    }, [updateUrl, url.preview, rowRefs])

    const handleExportConfirm = () => {
        if (exportMutation.isError && pendingExport) {
            void retryExport()
        } else {
            void confirmExport({
                total,
                filterSummary: listQuery.data?.filterSummary ?? "",
            })
        }
    }

    const columns = useSupplierOrdersListColumns({
        rows,
        focusedIndex,
        rowRefs,
        onPreview: openPreview,
        onQueryResult: handleQueryFromList,
        queryPending,
    })

    /** 表头说明：有筛选时读已生效条件摘要，无筛选时展示默认操作说明。 */
    const tableDescription = React.useMemo(() => {
        if (filters.appliedChips.length === 0) {
            return DEFAULT_TABLE_DESCRIPTION
        }
        return `当前筛选：${filters.appliedChips
            .map((chip) => chip.label)
            .join(" · ")}`
    }, [filters.appliedChips])

    return (
        <PageScaffold>
            <PageHeader
                title="供应商订单"
                actions={
                    <div className="flex flex-wrap items-center gap-2">
                        <DataFreshness
                            updatedAt="刚刚"
                            dateTime={listQuery.data?.queriedAt}
                            state={listQuery.isFetching ? "syncing" : "fresh"}
                            label="列表数据"
                        />
                        <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled={
                                !listQuery.data ||
                                total === 0 ||
                                exportMutation.isPending
                            }
                            onClick={openExportPreview}
                        >
                            <DownloadIcon className="size-3.5" />
                            导出
                        </Button>
                    </div>
                }
            />

            {returnTo ? (
                <SupplierOrdersListReturnBanner returnTo={returnTo} />
            ) : null}

            <SupplierOrdersListMetricStrip
                metrics={metrics}
                url={url}
                updateUrl={updateUrl}
            />

            {actionResult ? (
                <FormalActionResult
                    status={
                        actionResult.status === "failed"
                            ? "rejected"
                            : actionResult.status
                    }
                    title={actionResult.title}
                    description={actionResult.description}
                    reference={actionResult.reference}
                    actions={
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={dismissActionResult}
                        >
                            关闭
                        </Button>
                    }
                />
            ) : null}

            {exportResult ? (
                <SupplierOrdersListExportResult result={exportResult} />
            ) : null}

            {exportPreviewOpen ? (
                <SupplierOrdersListExportPreview
                    total={total}
                    filterSummary={listQuery.data?.filterSummary ?? "—"}
                    isPending={exportMutation.isPending}
                    isError={exportMutation.isError}
                    error={exportMutation.error}
                    isRetry={Boolean(exportMutation.isError && pendingExport)}
                    onConfirm={handleExportConfirm}
                    onCancel={closeExportPreview}
                />
            ) : null}

            <SupplierOrdersListTable
                toolbar={
                    <SupplierOrdersListToolbar
                        searchInputRef={searchInputRef}
                        view={url.view}
                        onViewChange={(view) => updateUrl({ view, page: 1 })}
                        searchDraft={filters.searchDraft}
                        onSearchDraftChange={filters.setSearchDraft}
                        panelOpen={filters.panelOpen}
                        setPanelOpen={filters.setPanelOpen}
                        hasStructuredFilters={filters.hasStructuredFilters}
                        appliedChips={filters.appliedChips}
                        onRemoveFilter={filters.removeFilter}
                        onApplyFilters={filters.applyFilters}
                        onClearAllFilters={filters.clearAllFilters}
                        onResetMoreFilters={filters.resetMoreFilters}
                        filterError={filters.filterError}
                        setFilterError={filters.setFilterError}
                        supplierIdDraft={filters.supplierIdDraft}
                        setSupplierIdDraft={filters.setSupplierIdDraft}
                        fulfillmentStatusesDraft={
                            filters.fulfillmentStatusesDraft
                        }
                        setFulfillmentStatusesDraft={
                            filters.setFulfillmentStatusesDraft
                        }
                        cancelStatusesDraft={filters.cancelStatusesDraft}
                        setCancelStatusesDraft={
                            filters.setCancelStatusesDraft
                        }
                        refundStatusesDraft={filters.refundStatusesDraft}
                        setRefundStatusesDraft={filters.setRefundStatusesDraft}
                        paidFromDraft={filters.paidFromDraft}
                        setPaidFromDraft={filters.setPaidFromDraft}
                        paidToDraft={filters.paidToDraft}
                        setPaidToDraft={filters.setPaidToDraft}
                    />
                }
                rows={rows}
                columns={columns}
                total={total}
                loading={listQuery.isPending}
                error={listQuery.isError ? listQuery.error : null}
                onRetry={() => void listQuery.refetch()}
                hasActiveFilters={filters.hasActiveFilters}
                onClearFilters={filters.clearAllFilters}
                description={tableDescription}
                sorting={sorting}
                onSortingChange={handleSortingChange}
                pagination={pagination}
                onPaginationChange={(next) => {
                    updateUrl({
                        page: next.pageIndex + 1,
                        pageSize: next.pageSize,
                    })
                }}
                onRowPreview={openPreview}
            />

            <SupplierOrdersListPreviewSheet
                open={Boolean(url.preview)}
                onOpenChange={(open) => {
                    if (!open) closePreview()
                }}
                preview={previewQuery.data}
                previewPending={previewQuery.isPending}
                onClose={closePreview}
                queryPending={queryPending}
                onQueryResult={queryFromPreview}
            />
        </PageScaffold>
    )
}
