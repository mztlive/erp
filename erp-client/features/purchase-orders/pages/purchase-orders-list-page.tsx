"use client"

import * as React from "react"
import { DownloadIcon, PlusIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessTableFrame,
    DataFreshness,
    DataTable,
    FormalActionResult,
    PageActions,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { usePurchaseOrdersListController } from "@/features/purchase-orders/hooks/use-purchase-orders-list-controller"
import { buildPurchaseOrdersListColumns } from "@/features/purchase-orders/pages/purchase-orders-list-columns"
import { PurchaseOrdersCreateDialog } from "@/features/purchase-orders/pages/purchase-orders-list-create-dialog"
import { PurchaseOrdersListToolbar } from "@/features/purchase-orders/pages/purchase-orders-list-toolbar"

export function PurchaseOrdersListPage() {
    const ctrl = usePurchaseOrdersListController()

    const columns = React.useMemo(
        () =>
            buildPurchaseOrdersListColumns({
                pageRows: ctrl.pageRows,
                focusedIndex: ctrl.focusedIndex,
                listReturnHref: ctrl.listReturnHref,
                rowRefs: ctrl.rowRefs,
            }),
        [ctrl.focusedIndex, ctrl.listReturnHref, ctrl.pageRows, ctrl.rowRefs],
    )

    if (ctrl.listQuery.isPending) {
        return (
            <PageScaffold density="compact">
                <PageHeader title="采购单" description="正在加载列表…" />
                <div className="h-24 animate-pulse rounded-lg bg-muted" />
                <div className="h-96 animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
        )
    }

    const { filters } = ctrl
    const listLoadFailed = ctrl.listQuery.isError
    // 表头说明：有筛选时展示 Applied 摘要，无筛选时展示默认操作说明
    const filterDescription = filters.hasActiveFilters
        ? `当前筛选：${filters.appliedChips.map((chip) => chip.label).join(" · ")}`
        : "搜索采购单号、供应商或来源销售单；键盘 j/k 移动行，Enter 打开详情，/ 聚焦搜索。"

    return (
        <PageScaffold density="compact">
            <PageHeader
                title="采购单"
                metadata={
                    <DataFreshness
                        updatedAt={
                            ctrl.listQuery.data?.freshness.updatedAt
                                ? new Date(
                                      ctrl.listQuery.data.freshness.updatedAt,
                                  ).toLocaleString("zh-CN", { hour12: false })
                                : "刚刚"
                        }
                        dateTime={ctrl.listQuery.data?.freshness.updatedAt}
                        state="fresh"
                        label="列表数据"
                    />
                }
                actions={
                    <PageActions
                        actions={[
                            {
                                actionKey: "export",
                                label: "导出",
                                icon: DownloadIcon,
                                variant: "outline",
                                mobileVisibility: "hide",
                                disabled: ctrl.total === 0,
                                onClick: () => void ctrl.exportCsv(),
                            },
                            {
                                actionKey: "create",
                                label: "新建采购单",
                                icon: PlusIcon,
                                onClick: ctrl.openCreateDialog,
                            },
                        ]}
                    />
                }
            />

            {ctrl.actionResult ? (
                <FormalActionResult
                    status={
                        ctrl.actionResult.status === "failed"
                            ? "rejected"
                            : ctrl.actionResult.status === "unknown"
                              ? "unknown"
                              : "succeeded"
                    }
                    title={ctrl.actionResult.title}
                    description={ctrl.actionResult.description}
                    reference={ctrl.actionResult.reference}
                    actions={
                        <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() => ctrl.setActionResult(null)}
                        >
                            关闭
                        </Button>
                    }
                />
            ) : null}

            <BusinessTableFrame
                showHeader
                title={
                    <span className="inline-flex items-baseline gap-2">
                        采购单列表
                        <span
                            className="font-normal text-muted-foreground"
                            aria-live="polite"
                        >
                            {ctrl.total} 条
                        </span>
                    </span>
                }
                description={filterDescription}
                toolbar={
                    <PurchaseOrdersListToolbar
                        searchInputRef={ctrl.searchInputRef}
                        {...filters}
                    />
                }
                table={
                    <DataTable
                        data={ctrl.pageRows}
                        columns={columns}
                        getRowId={(row) => row.purchaseOrderId}
                        rowCount={ctrl.total}
                        pagination={ctrl.pagination}
                        onPaginationChange={(next) => {
                            ctrl.pushUrl({
                                page: next.pageIndex + 1,
                                pageSize: next.pageSize,
                            })
                        }}
                        sorting={ctrl.sorting}
                        onSortingChange={(next) => {
                            const nextSort = next[0]
                            ctrl.pushUrl({
                                sort: nextSort
                                    ? `${nextSort.id}:${nextSort.desc ? "desc" : "asc"}`
                                    : undefined,
                                page: 1,
                            })
                        }}
                        layout="flush"
                        loading={ctrl.listQuery.isFetching}
                        defaultColumnPinning={{
                            left: ["document"],
                        }}
                        onRowOpen={(row) =>
                            ctrl.openDetail(row.purchaseOrderId)
                        }
                        errorState={
                            listLoadFailed ? (
                                <BusinessFailureState
                                    error={ctrl.listQuery.error}
                                    onRetry={() =>
                                        void ctrl.listQuery.refetch()
                                    }
                                />
                            ) : undefined
                        }
                        emptyState={
                            !listLoadFailed && ctrl.pageRows.length === 0 ? (
                                <BusinessEmptyState
                                    kind={
                                        filters.hasActiveFilters
                                            ? "filter"
                                            : "no-data"
                                    }
                                    className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                    title={
                                        filters.hasActiveFilters
                                            ? "当前筛选无结果"
                                            : "暂无采购单"
                                    }
                                    description={
                                        filters.hasActiveFilters
                                            ? "没有记录符合当前筛选条件，可清除筛选后重试。"
                                            : "还没有采购单，可新建采购单。"
                                    }
                                    action={
                                        filters.hasActiveFilters ? (
                                            <Button
                                                type="button"
                                                variant="secondary"
                                                size="sm"
                                                className="rounded-lg shadow-none"
                                                onClick={
                                                    filters.clearAllFilters
                                                }
                                            >
                                                清除筛选
                                            </Button>
                                        ) : (
                                            <Button
                                                type="button"
                                                variant="secondary"
                                                size="sm"
                                                className="rounded-lg shadow-none"
                                                onClick={ctrl.openCreateDialog}
                                            >
                                                新建采购单
                                            </Button>
                                        )
                                    }
                                />
                            ) : undefined
                        }
                    />
                }
            />

            <PurchaseOrdersCreateDialog
                open={ctrl.createOpen}
                onOpenChange={ctrl.setCreateOpen}
                openBases={ctrl.openBases}
                basesPending={ctrl.basesQuery.isLoading}
                basesFailed={
                    ctrl.basesQuery.isError || ctrl.basesQuery.isRefetchError
                }
                basesError={ctrl.basesQuery.error}
                onRetryBases={() => void ctrl.basesQuery.refetch()}
                basisFromUrl={ctrl.basisFromUrl}
                salesOrderFromUrl={ctrl.salesOrderFromUrl}
                selectedBasisId={ctrl.selectedBasisId}
                onSelectedBasisIdChange={ctrl.setSelectedBasisId}
                createPending={ctrl.createMutation.isPending}
                createResult={ctrl.actionResult}
                onCreate={ctrl.handleCreate}
            />
        </PageScaffold>
    )
}
