"use client"

import type {
    ColumnDef,
    PaginationState,
    SortingState,
} from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DataTable,
} from "@/components/business"
import { listWorkspaceEmptyStateClassName } from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import type { SupplierOrderListRow } from "@/features/supplier-orders/types"

export type SupplierOrdersListTableProps = {
    rows: SupplierOrderListRow[]
    columns: ColumnDef<SupplierOrderListRow>[]
    total: number
    loading: boolean
    error: Error | null
    onRetry: () => void
    hasActiveFilters: boolean
    noScope?: boolean
    onClearFilters: () => void
    sorting: SortingState
    onSortingChange: (next: SortingState) => void
    pagination: PaginationState
    onPaginationChange: (next: PaginationState) => void
    onRowPreview: (orderId: string) => void
}

export function SupplierOrdersListTable({
    rows,
    columns,
    total,
    loading,
    error,
    onRetry,
    hasActiveFilters,
    noScope = false,
    onClearFilters,
    sorting,
    onSortingChange,
    pagination,
    onPaginationChange,
    onRowPreview,
}: SupplierOrdersListTableProps) {
    return (
        <DataTable
            id="supplier-orders-list-table"
            data={rows}
            columns={columns}
            getRowId={(row) => row.orderId}
            rowCount={total}
            loading={loading}
            errorState={
                error ? (
                    <BusinessFailureState
                        title="供应商订单列表加载失败"
                        error={error}
                        action={
                            <Button
                                id="supplier-orders-list-table-error-retry"
                                type="button"
                                size="sm"
                                onClick={onRetry}
                            >
                                重试
                            </Button>
                        }
                    />
                ) : undefined
            }
            emptyState={
                !loading && rows.length === 0 ? (
                    <BusinessEmptyState
                        kind={
                            noScope
                                ? "no-data"
                                : hasActiveFilters
                                  ? "filter"
                                  : "no-data"
                        }
                        className={listWorkspaceEmptyStateClassName}
                        title={
                            noScope
                                ? "当前没有可查看的供应商订单范围"
                                : hasActiveFilters
                                  ? "当前筛选无结果"
                                  : "当前范围没有供应商订单"
                        }
                        description={
                            noScope
                                ? "已授权动作但没有可见订单。请联系管理员配置数据范围，或清除筛选后重试。"
                                : hasActiveFilters
                                  ? "没有记录符合当前筛选条件，可清除筛选后重试。"
                                  : "调整视图、供应商或支付时间后重试。"
                        }
                        action={
                            hasActiveFilters ? (
                                <Button
                                    id="supplier-orders-list-table-empty-clear"
                                    type="button"
                                    size="sm"
                                    variant="secondary"
                                    className="rounded-lg shadow-none"
                                    onClick={onClearFilters}
                                >
                                    清除筛选
                                </Button>
                            ) : undefined
                        }
                    />
                ) : undefined
            }
            sorting={sorting}
            onSortingChange={onSortingChange}
            pagination={pagination}
            onPaginationChange={onPaginationChange}
            layout="flush"
            defaultColumnPinning={{
                left: ["identity"],
                right: ["actions"],
            }}
            onRowPreview={(row) => onRowPreview(row.orderId)}
            onRowOpen={(row) => onRowPreview(row.orderId)}
        />
    )
}
