"use client"

import * as React from "react"
import { DownloadIcon, PlusIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DataTable,
    FormalActionResult,
    PageActions,
    PageScaffold,
} from "@/components/business"
import {
    ListWorkSurface,
    ListWorkspaceHeader,
    ListWorkspaceViews,
    listWorkspaceEmptyStateClassName,
    listWorkspaceStyles as styles,
} from "@/components/business/list-workspace"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import { usePurchaseOrdersListController } from "@/features/purchase-orders/hooks/use-purchase-orders-list-controller"
import { buildPurchaseOrdersListColumns } from "@/features/purchase-orders/pages/purchase-orders-list-columns"
import { PurchaseOrdersListToolbar } from "@/features/purchase-orders/pages/purchase-orders-list-toolbar"
import {
    PO_STATUS_FILTER_LABEL,
    type PurchaseOrderStatusFilter,
} from "@/features/purchase-orders/types"

const PURCHASE_ORDER_VIEWS = Object.entries(PO_STATUS_FILTER_LABEL) as Array<
    [PurchaseOrderStatusFilter, string]
>

export function PurchaseOrdersListPage() {
    const ctrl = usePurchaseOrdersListController()
    const exportPending = ctrl.exportQuery.isFetching

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

    const { filters } = ctrl
    const listLoadFailed = ctrl.listQuery.isError
    const updatedAt = ctrl.listQuery.data?.freshness.updatedAt

    return (
        <PageScaffold density="compact" className={styles.page}>
            <ListWorkspaceHeader
                eyebrow="采购"
                title="采购单"
                description={
                    <>
                        查看采购单、履约进度与金额。
                        <span className="ml-3 text-xs" role="status">
                            {listLoadFailed ? (
                                "查询失败"
                            ) : ctrl.listQuery.isFetching ? (
                                "正在更新…"
                            ) : updatedAt ? (
                                <time dateTime={updatedAt}>
                                    更新于{" "}
                                    {new Date(updatedAt).toLocaleTimeString(
                                        "zh-CN",
                                        {
                                            hour: "2-digit",
                                            minute: "2-digit",
                                            hour12: false,
                                        },
                                    )}
                                </time>
                            ) : (
                                "正在查询"
                            )}
                        </span>
                    </>
                }
            >
                <PageActions
                    actions={[
                        {
                            actionKey: "export",
                            label: exportPending ? (
                                <>
                                    <Spinner
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    导出中…
                                </>
                            ) : (
                                "导出"
                            ),
                            icon: exportPending ? undefined : DownloadIcon,
                            variant: "outline",
                            mobileVisibility: "hide",
                            disabled: exportPending || ctrl.total === 0,
                            onClick: () => void ctrl.exportCsv(),
                            id: "procurement-orders-list-export",
                        },
                        {
                            actionKey: "create",
                            label: "新建采购单",
                            icon: PlusIcon,
                            onClick: ctrl.openCreatePage,
                            id: "procurement-orders-list-create",
                        },
                    ]}
                />
            </ListWorkspaceHeader>

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
                            id="procurement-orders-list-result-close"
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

            <ListWorkSurface
                ariaLabel="采购单列表"
                views={
                    <ListWorkspaceViews
                        ariaLabel="采购单视图"
                        hint="选择采购单查看详情"
                        items={PURCHASE_ORDER_VIEWS.map(([status, label]) => ({
                            id: `procurement-orders-list-view-${toAutomationIdSegment(status)}`,
                            label: status === "all" ? "全部采购单" : label,
                            active: ctrl.statusFilter === status,
                            onClick: () =>
                                ctrl.pushUrl({
                                    status,
                                    metric: "all",
                                    page: 1,
                                }),
                        }))}
                    />
                }
                toolbar={
                    <PurchaseOrdersListToolbar
                        searchInputRef={ctrl.searchInputRef}
                        searchDraft={filters.searchDraft}
                        setSearchDraft={filters.setSearchDraft}
                        appliedChips={filters.appliedChips}
                        removeFilter={filters.removeFilter}
                        applyFilters={filters.applyFilters}
                        clearAllFilters={filters.clearAllFilters}
                        hasPendingChanges={filters.hasPendingChanges}
                        resultCount={
                            ctrl.listQuery.data ? ctrl.total : undefined
                        }
                        loading={ctrl.listQuery.isFetching}
                        failed={listLoadFailed}
                    />
                }
                table={
                    <DataTable
                        id="procurement-orders-list-table"
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
                                    action={
                                        <Button
                                            id="procurement-orders-list-retry"
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            onClick={() =>
                                                void ctrl.listQuery.refetch()
                                            }
                                        >
                                            重试
                                        </Button>
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
                                    className={listWorkspaceEmptyStateClassName}
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
                                                id="procurement-orders-list-clear-filters"
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
                                                id="procurement-orders-list-empty-create"
                                                type="button"
                                                variant="secondary"
                                                size="sm"
                                                className="rounded-lg shadow-none"
                                                onClick={ctrl.openCreatePage}
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
        </PageScaffold>
    )
}
