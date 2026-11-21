"use client"

import * as React from "react"
import { DownloadIcon, PlusIcon } from "lucide-react"

import { FormalActionResult } from "@/components/business"
import {
    ListDataTable,
    ListWorkspacePage,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { ExpensePurchaseStartDialog } from "@/features/purchase-orders/components/expense-purchase-start-dialog"
import { usePurchaseOrdersListController } from "@/features/purchase-orders/hooks/use-purchase-orders-list-controller"
import { displayPurchaseOrderNo } from "@/features/purchase-orders/lib/purchase-orders-list-helpers"
import { buildPurchaseOrdersListColumns } from "@/features/purchase-orders/pages/purchase-orders-list-columns"
import { PurchaseOrdersListToolbar } from "@/features/purchase-orders/pages/purchase-orders-list-toolbar"
import {
    PO_STATUS_FILTER_LABEL,
    type PurchaseOrderStatusFilter,
} from "@/features/purchase-orders/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { hasPermission } from "@/lib/permissions"

const PREFIX = "procurement-orders-list"
const PAGE_SIZES = [20, 50, 100]
// 窄窗只保留身份列冻结，避免两侧固定列遮住表体。
const TABLE_CLASS =
    "max-sm:[&_[data-column-id=source][data-column-pinned=right]]:right-auto! max-sm:[&_[data-column-id=source][data-column-pinned=right]]:shadow-none!"
const PURCHASE_ORDER_VIEWS = Object.entries(PO_STATUS_FILTER_LABEL) as Array<
    [PurchaseOrderStatusFilter, string]
>

export function PurchaseOrdersListPage() {
    const [expenseOpen, setExpenseOpen] = React.useState(false)
    const profile = useAccountProfileQuery()
    const permissions = profile.isError ? undefined : profile.data?.permissions
    const canCreate = hasPermission(permissions, "purchase_order:create")
    const canExport = hasPermission(permissions, "purchase_order:list")
    const canOpen = hasPermission(permissions, "purchase_order:detail")
    const canOpenSource = hasPermission(permissions, "sales_order:detail")
    const ctrl = usePurchaseOrdersListController()
    const { filters, listQuery } = ctrl
    const exportPending = ctrl.exportQuery.isFetching
    const [hasLoaded, setHasLoaded] = React.useState(false)
    React.useEffect(() => {
        if (!listQuery.isPending) setHasLoaded(true)
    }, [listQuery.isPending])
    const listLoadFailed =
        listQuery.isError || (!listQuery.isPending && !listQuery.data)
    const noScope = listQuery.data?.emptyReason === "no_scope"
    const sourceOrderNo = ctrl.salesOrderFromUrl
        ? ctrl.pageRows.find(
              (row) => row.salesOrderId === ctrl.salesOrderFromUrl,
          )?.salesOrderNo
        : undefined

    const columns = React.useMemo(
        () =>
            buildPurchaseOrdersListColumns({
                listReturnHref: ctrl.listReturnHref,
                canOpen,
                canOpenSource,
            }),
        [canOpen, canOpenSource, ctrl.listReturnHref],
    )

    return (
        <ListWorkspacePage
            title="采购单"
            loading={listQuery.isPending && !hasLoaded}
            views={{
                ariaLabel: "采购单视图",
                items: PURCHASE_ORDER_VIEWS.map(([status, label]) => ({
                    id: `${PREFIX}-view-${toAutomationIdSegment(status)}`,
                    label: status === "all" ? "全部采购单" : label,
                    active: ctrl.statusFilter === status,
                    onClick: () =>
                        ctrl.pushUrl({ status, metric: "all", page: 1 }),
                })),
            }}
            actions={[
                {
                    id: `${PREFIX}-create-expense`,
                    actionKey: "create-expense",
                    label: "新建费用采购单",
                    icon: PlusIcon,
                    tier: "quiet",
                    hidden: !canCreate,
                    onClick: () => setExpenseOpen(true),
                },
                {
                    id: `${PREFIX}-export`,
                    actionKey: "export",
                    label: exportPending ? "导出中…" : "导出",
                    icon: DownloadIcon,
                    tier: "quiet",
                    hidden: !canExport,
                    loading: exportPending,
                    disabled:
                        exportPending || ctrl.total === 0 || listLoadFailed,
                    onClick: () => void ctrl.exportCsv(),
                },
                {
                    id: `${PREFIX}-create`,
                    actionKey: "create",
                    label: "新建采购单",
                    icon: PlusIcon,
                    tier: "primary",
                    variant: filters.hasPendingChanges
                        ? "secondary"
                        : "default",
                    hidden: !canCreate,
                    onClick: ctrl.openCreatePage,
                },
            ]}
            alerts={
                ctrl.actionResult ? (
                    <FormalActionResult
                        status={
                            ctrl.actionResult.status === "failed"
                                ? "rejected"
                                : ctrl.actionResult.status
                        }
                        title={ctrl.actionResult.title}
                        description={ctrl.actionResult.description}
                        reference={ctrl.actionResult.reference}
                        actions={
                            <Button
                                id={`${PREFIX}-result-close`}
                                type="button"
                                variant="ghost"
                                size="sm"
                                onClick={() => ctrl.setActionResult(null)}
                            >
                                关闭
                            </Button>
                        }
                    />
                ) : null
            }
            surface={{
                ariaLabel: "采购单列表",
                tableClassName: TABLE_CLASS,
                toolbar: (
                    <PurchaseOrdersListToolbar
                        filters={filters}
                        searchInputRef={ctrl.searchInputRef}
                        sourceOrderNo={sourceOrderNo}
                        loading={listQuery.isPending || listQuery.isFetching}
                        failed={listLoadFailed}
                    />
                ),
                table: (
                    <ListDataTable
                        id={`${PREFIX}-table`}
                        entityName="采购单"
                        data={ctrl.pageRows}
                        columns={columns}
                        defaultColumnOrder={[
                            "document",
                            "type",
                            "status",
                            "tracks",
                            "amount",
                            "owner",
                            "paymentTerm",
                            "source",
                        ]}
                        defaultColumnVisibility={{ paymentTerm: false }}
                        defaultColumnPinning={{
                            left: ["document"],
                            right: canOpenSource ? ["source"] : [],
                        }}
                        getRowId={(row) => row.purchaseOrderId}
                        rowLabel={(row) =>
                            `${row.supplierName}（${displayPurchaseOrderNo(row)}）`
                        }
                        rowCount={ctrl.total}
                        pagination={ctrl.pagination}
                        pageSizeOptions={
                            PAGE_SIZES.includes(ctrl.pagination.pageSize)
                                ? PAGE_SIZES
                                : [
                                      ...PAGE_SIZES,
                                      ctrl.pagination.pageSize,
                                  ].sort((a, b) => a - b)
                        }
                        onPaginationChange={(next) => {
                            ctrl.pushUrl({
                                page: next.pageIndex + 1,
                                pageSize: next.pageSize,
                            })
                        }}
                        sorting={ctrl.sorting}
                        onSortingChange={(next) => {
                            const order = next[0]
                            ctrl.pushUrl({
                                sort: order
                                    ? `${order.id}:${order.desc ? "desc" : "asc"}`
                                    : undefined,
                                page: 1,
                            })
                        }}
                        showToolbarSummary={false}
                        showPagination={!listLoadFailed}
                        loading={listQuery.isPending || listQuery.isFetching}
                        onRowOpen={
                            canOpen
                                ? (row) => ctrl.openDetail(row.purchaseOrderId)
                                : undefined
                        }
                        isError={listLoadFailed}
                        error={listQuery.error}
                        onRetry={() => void listQuery.refetch()}
                        retryButtonId={`${PREFIX}-retry`}
                        noScope={noScope}
                        hasActiveFilters={filters.hasActiveFilters}
                        onClearFilters={
                            noScope ? undefined : filters.clearAllFilters
                        }
                        clearFiltersButtonId={`${PREFIX}-clear-filters`}
                        canCreate={canCreate}
                        onCreate={ctrl.openCreatePage}
                        createButtonId={`${PREFIX}-empty-create`}
                        createLabel="新建采购单"
                        emptyCopy={{
                            noScopeTitle: "暂无采购单数据范围",
                            noScopeDescription:
                                "请联系管理员配置可查看的业务范围。",
                            emptyTitle: "暂无采购单",
                            emptyDescription: canCreate
                                ? "当前业务范围内还没有采购单，可新建第一张单。"
                                : "当前业务范围内还没有采购单。",
                        }}
                    />
                ),
            }}
        >
            {canCreate ? (
                <ExpensePurchaseStartDialog
                    open={expenseOpen}
                    onOpenChange={setExpenseOpen}
                    onCreated={(created) =>
                        ctrl.openDetail(created.purchaseOrderId)
                    }
                />
            ) : null}
        </ListWorkspacePage>
    )
}
