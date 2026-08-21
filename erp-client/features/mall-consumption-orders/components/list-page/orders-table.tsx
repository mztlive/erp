"use client"

import type {
    ColumnDef,
    ColumnPinningState,
    PaginationState,
} from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DataTable,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type {
    EmptyReason,
    MallConsumptionOrderRow,
} from "@/features/mall-consumption-orders/types"

type Props = {
    periodSelected: boolean
    isPending: boolean
    isError: boolean
    error: unknown
    empty: EmptyReason | undefined
    rows: MallConsumptionOrderRow[]
    columns: ColumnDef<MallConsumptionOrderRow>[]
    columnPinning: ColumnPinningState
    pagination: PaginationState
    rowCount: number
    isFetching: boolean
    onPaginationChange: (next: PaginationState) => void
    onRowPreview: (row: MallConsumptionOrderRow) => void
    onClearFilters: () => void
    onRetry: () => void
}

export function ConsumptionOrdersTable({
    periodSelected,
    isPending,
    isError,
    error,
    empty,
    rows,
    columns,
    columnPinning,
    pagination,
    rowCount,
    isFetching,
    onPaginationChange,
    onRowPreview,
    onClearFilters,
    onRetry,
}: Props) {
    if (!periodSelected) {
        return (
            <BusinessEmptyState
                kind="filter"
                title="请选择记录发生起止时间"
                description="默认期间策略未配置：请在「更多筛选」中选择完整的事实发生起止时间后再查询，不静默拉取全量记录。"
                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
            />
        )
    }
    if (isPending) {
        return <div className="h-64 animate-pulse rounded-lg bg-muted" aria-busy />
    }
    if (isError) {
        return (
            <BusinessFailureState
                title="查询失败"
                error={error}
                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                action={
                    <Button
                        type="button"
                        variant="secondary"
                        size="sm"
                        className="rounded-lg shadow-none"
                        onClick={() => onRetry()}
                    >
                        重试
                    </Button>
                }
            />
        )
    }
    if (empty === "FILTER_EMPTY") {
        return (
            <BusinessEmptyState
                kind="filter"
                title="当前筛选无结果"
                description="可调整期间、商城、履约链、归集、支付方式、成本口径、记录类型、供应商状态、数据来源或搜索条件后重试。"
                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                action={
                    <Button
                        type="button"
                        variant="secondary"
                        size="sm"
                        className="rounded-lg shadow-none"
                        onClick={onClearFilters}
                    >
                        清除筛选
                    </Button>
                }
            />
        )
    }
    if (empty === "NO_DATA" || rows.length === 0) {
        return (
            <BusinessEmptyState
                kind="no-data"
                title="当前范围没有消费订单"
                description="新支付记录到达后会自动显示。"
                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
            />
        )
    }
    return (
        <DataTable
            data={rows}
            columns={columns}
            getRowId={(row) => row.mallOrderId}
            layout="flush"
            enableColumnPinning
            columnPinning={columnPinning}
            pagination={pagination}
            onPaginationChange={onPaginationChange}
            rowCount={rowCount}
            manualPagination
            loading={isFetching}
            onRowPreview={(row) => onRowPreview(row)}
            onRowOpen={(row) => onRowPreview(row)}
            showPagination
            pageSizeOptions={[8, 10, 20]}
        />
    )
}
