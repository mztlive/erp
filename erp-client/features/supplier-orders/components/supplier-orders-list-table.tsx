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
    /** 结果卡可见说明：有筛选时为人读摘要，否则为默认操作说明。 */
    description: string
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
    description,
    sorting,
    onSortingChange,
    pagination,
    onPaginationChange,
    onRowPreview,
}: SupplierOrdersListTableProps) {
    return (
        <BusinessTableFrame
            showHeader
            title={
                <span className="inline-flex items-baseline gap-2">
                    供应商订单
                    <span
                        className="font-normal text-muted-foreground"
                        aria-live="polite"
                    >
                        {total.toLocaleString("zh-CN")} 条
                    </span>
                </span>
            }
            description={description}
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
                                kind={
                                    hasActiveFilters ? "filter" : "no-data"
                                }
                                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                title={
                                    hasActiveFilters
                                        ? "当前筛选无结果"
                                        : "当前范围没有供应商订单"
                                }
                                description={
                                    hasActiveFilters
                                        ? "没有记录符合当前筛选条件，可清除筛选后重试。"
                                        : "调整视图、供应商或支付时间，或从商城消费订单钻取。"
                                }
                                action={
                                    hasActiveFilters ? (
                                        <Button
                                            type="button"
                                            size="sm"
                                            variant="secondary"
                                            className="rounded-lg shadow-none"
                                            onClick={onClearFilters}
                                        >
                                            清除筛选
                                        </Button>
                                    ) : (
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
                                    )
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
