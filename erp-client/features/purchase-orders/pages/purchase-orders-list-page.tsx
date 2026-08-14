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
    MetricFilterItem,
    MetricStrip,
    PageActions,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { usePurchaseOrdersListController } from "@/features/purchase-orders/hooks/use-purchase-orders-list-controller"
import { buildPurchaseOrdersListColumns } from "@/features/purchase-orders/pages/purchase-orders-list-columns"
import { PurchaseOrdersCreateDialog } from "@/features/purchase-orders/pages/purchase-orders-list-create-dialog"
import { PurchaseOrdersListPreviewSheet } from "@/features/purchase-orders/pages/purchase-orders-list-preview-sheet"
import { PurchaseOrdersListToolbar } from "@/features/purchase-orders/pages/purchase-orders-list-toolbar"
import {
    PO_METRIC_LABEL,
    PO_STATUS_FILTER_LABEL,
    type PurchaseOrderMetricFilter,
} from "@/features/purchase-orders/types"

export function PurchaseOrdersListPage() {
    const ctrl = usePurchaseOrdersListController()

    const columns = React.useMemo(
        () =>
            buildPurchaseOrdersListColumns({
                pageRows: ctrl.pageRows,
                focusedIndex: ctrl.focusedIndex,
                listReturnHref: ctrl.listReturnHref,
                rowRefs: ctrl.rowRefs,
                onPreview: ctrl.setPreviewId,
            }),
        [
            ctrl.focusedIndex,
            ctrl.listReturnHref,
            ctrl.pageRows,
            ctrl.rowRefs,
            ctrl.setPreviewId,
        ],
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
                breadcrumbs={[
                    {
                        id: "proc",
                        label: "采购与履约",
                        href: "/procurement/confirm",
                    },
                    { id: "orders", label: "采购单", current: true },
                ]}
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

            <MetricStrip
                columns={
                    Math.min(4, Math.max(2, ctrl.metrics.length)) as 2 | 3 | 4
                }
                aria-label="采购单指标筛选"
            >
                {ctrl.metrics.map((metric) => (
                    <MetricFilterItem
                        key={metric.key}
                        label={metric.label}
                        value={metric.count}
                        detail={metric.detail}
                        active={
                            metric.key !== "pending_create" &&
                            ctrl.metricKey === metric.key
                        }
                        onClick={() => {
                            if (metric.key === "pending_create") {
                                ctrl.openCreateDialog()
                                return
                            }
                            ctrl.pushUrl({
                                metric: metric.key as PurchaseOrderMetricFilter,
                                page: 1,
                            })
                        }}
                    />
                ))}
            </MetricStrip>

            <BusinessTableFrame
                title="采购单列表"
                description={
                    ctrl.metricKey === "all" && ctrl.statusFilter === "all"
                        ? "搜索采购单号、供应商或来源销售单；键盘 j/k 移动行，Enter 打开预览，/ 聚焦搜索。"
                        : `当前筛选：${PO_METRIC_LABEL[ctrl.effectiveMetric]} · ${PO_STATUS_FILTER_LABEL[ctrl.statusFilter]}`
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
                        density="compact"
                        loading={ctrl.listQuery.isFetching}
                        defaultColumnPinning={{
                            left: ["document"],
                            right: ["actions"],
                        }}
                        onRowPreview={(row) =>
                            ctrl.setPreviewId(row.purchaseOrderId)
                        }
                        onRowOpen={(row) =>
                            ctrl.setPreviewId(row.purchaseOrderId)
                        }
                        errorState={
                            <BusinessFailureState
                                kind="system"
                                title="列表加载失败"
                                description="未能加载采购单列表，请重试；若持续失败可稍后再来。"
                                onRetry={() => void ctrl.listQuery.refetch()}
                            />
                        }
                        emptyTitle={
                            ctrl.hasActiveFilters
                                ? "没有符合条件的采购单"
                                : undefined
                        }
                        emptyDescription="当前筛选没有匹配的采购单，可调整或清除筛选后重试。"
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

            <PurchaseOrdersListPreviewSheet
                previewId={ctrl.previewId}
                onOpenChange={(open) => {
                    if (!open) {
                        ctrl.closePreviewAndRefocus(ctrl.previewId)
                    }
                }}
                onClosePreview={ctrl.closePreviewAndRefocus}
                order={ctrl.previewQuery.data}
                pending={ctrl.previewQuery.isPending}
                listReturnHref={ctrl.listReturnHref}
            />

            <PurchaseOrdersCreateDialog
                open={ctrl.createOpen}
                onOpenChange={ctrl.setCreateOpen}
                openBases={ctrl.openBases}
                basisFromUrl={ctrl.basisFromUrl}
                selectedBasisId={ctrl.selectedBasisId}
                onSelectedBasisIdChange={ctrl.setSelectedBasisId}
                createPending={ctrl.createMutation.isPending}
                onCreate={() => void ctrl.handleCreate()}
            />
        </PageScaffold>
    )
}
