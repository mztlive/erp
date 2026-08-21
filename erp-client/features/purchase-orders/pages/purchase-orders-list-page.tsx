"use client"

import * as React from "react"
import Link from "next/link"
import { DownloadIcon, PlusIcon } from "lucide-react"

import {
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
import { PO_STATUS_FILTER_LABEL } from "@/features/purchase-orders/types"

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

    if (ctrl.listQuery.isError) {
        return (
            <PageScaffold density="compact">
                <PageHeader
                    title="采购单"
                    description="列表加载失败"
                    actions={
                        <Button
                            type="button"
                            onClick={() => void ctrl.listQuery.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
                <BusinessFailureState
                    title="列表加载失败"
                    error={ctrl.listQuery.error}
                    onRetry={() => void ctrl.listQuery.refetch()}
                />
            </PageScaffold>
        )
    }

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
                                mobileVisibility: "hide",
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
                title="采购单列表"
                description={
                    ctrl.statusFilter === "all"
                        ? "搜索采购单号、供应商或来源销售单；键盘 j/k 移动行，Enter 打开详情，/ 聚焦搜索。"
                        : `当前筛选：${PO_STATUS_FILTER_LABEL[ctrl.statusFilter]}`
                }
                toolbar={
                    <PurchaseOrdersListToolbar
                        searchDraft={ctrl.searchDraft}
                        onSearchDraftChange={ctrl.setSearchDraft}
                        statusFilter={ctrl.statusFilter}
                        onStatusFilterChange={(value) =>
                            ctrl.pushUrl({ status: value, page: 1 })
                        }
                        total={ctrl.total}
                        hasActiveFilters={ctrl.hasActiveFilters}
                        onClearFilters={ctrl.clearFilters}
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
                            ctrl.listQuery.isError ? (
                                <BusinessFailureState
                                    kind="system"
                                    title="列表加载失败"
                                    description="未能加载采购单列表，请重试；若持续失败可稍后再来。"
                                    onRetry={() =>
                                        void ctrl.listQuery.refetch()
                                    }
                                />
                            ) : undefined
                        }
                        emptyTitle={
                            ctrl.hasActiveFilters
                                ? "没有符合条件的采购单"
                                : "暂无采购单"
                        }
                        emptyDescription={
                            ctrl.hasActiveFilters
                                ? "当前筛选没有匹配的采购单，可调整或清除筛选后重试。"
                                : "还没有采购单。可从采购二次确认的创建依据新建。"
                        }
                        emptyAction={
                            <div className="flex flex-wrap gap-2">
                                {ctrl.hasActiveFilters ? (
                                    <Button
                                        type="button"
                                        variant="secondary"
                                        size="sm"
                                        className="rounded-lg shadow-none"
                                        onClick={ctrl.clearFilters}
                                    >
                                        清除筛选
                                    </Button>
                                ) : (
                                    <Button
                                        type="button"
                                        variant="secondary"
                                        size="sm"
                                        className="rounded-lg shadow-none"
                                        render={
                                            <Link href="/procurement/confirm" />
                                        }
                                    >
                                        去采购二次确认
                                    </Button>
                                )}
                            </div>
                        }
                    />
                }
            />

            <PurchaseOrdersCreateDialog
                open={ctrl.createOpen}
                onOpenChange={ctrl.setCreateOpen}
                openBases={ctrl.openBases}
                basesPending={ctrl.basesQuery.isLoading}
                basesFailed={ctrl.basesQuery.isError}
                onRetryBases={() => void ctrl.basesQuery.refetch()}
                basisFromUrl={ctrl.basisFromUrl}
                selectedBasisId={ctrl.selectedBasisId}
                onSelectedBasisIdChange={ctrl.setSelectedBasisId}
                createPending={ctrl.createMutation.isPending}
                onCreate={() => void ctrl.handleCreate()}
            />
        </PageScaffold>
    )
}
