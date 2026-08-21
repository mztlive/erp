"use client"

import Link from "next/link"
import type {
    ColumnDef,
    PaginationState,
    SortingState,
} from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessTableFrame,
    DataTable,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type { SupplierOrderListRow } from "@/features/supplier-orders/types"

export type SupplierOrdersListTableProps = {
    toolbar: React.ReactNode
    rows: SupplierOrderListRow[]
    columns: ColumnDef<SupplierOrderListRow>[]
    total: number
    loading: boolean
    error: Error | null
    onRetry: () => void
    hasActiveFilters: boolean
    onClearFilters: () => void
    sorting: SortingState
    onSortingChange: (next: SortingState) => void
    pagination: PaginationState
    onPaginationChange: (next: PaginationState) => void
    onRowPreview: (orderId: string) => void
}

export function SupplierOrdersListTable({
    toolbar,
    rows,
    columns,
    total,
    loading,
    error,
    onRetry,
    hasActiveFilters,
    onClearFilters,
    sorting,
    onSortingChange,
    pagination,
    onPaginationChange,
    onRowPreview,
}: SupplierOrdersListTableProps) {
    return (
        <BusinessTableFrame
            title="供应商订单列表"
            description="身份列与操作列固定；履约/取消/退款三种状态独立展示。"
            toolbar={toolbar}
            table={
                <DataTable
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
                                kind="filter"
                                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                title="当前范围没有供应商订单"
                                description="调整视图、供应商或支付时间，或从商城消费订单钻取。"
                                action={
                                    <div className="flex flex-wrap gap-2">
                                        {hasActiveFilters ? (
                                            <Button
                                                type="button"
                                                size="sm"
                                                variant="secondary"
                                                className="rounded-lg shadow-none"
                                                onClick={onClearFilters}
                                            >
                                                清除筛选
                                            </Button>
                                        ) : null}
                                        <Button
                                            type="button"
                                            size="sm"
                                            variant="secondary"
                                            className="rounded-lg shadow-none"
                                            render={
                                                <Link href="/commerce/consumption-orders" />
                                            }
                                        >
                                            打开商城消费订单
                                        </Button>
                                    </div>
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
            }
        />
    )
}
