import type {
    ColumnDef,
    PaginationState,
    SortingState,
} from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessTableFrame,
    DataTable,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type { CardBusinessAnalyticsView, CardBusinessRow } from "../types"

export interface CardBusinessDrillTableProps {
    data: CardBusinessAnalyticsView
    columns: ColumnDef<CardBusinessRow>[]
    pagination: PaginationState
    tableSorting: SortingState
    onPaginationChange: (next: PaginationState) => void
    onSortingChange: (next: SortingState) => void
    onClearFilters: () => void
}

/** 下钻明细：空态或服务端分页/排序的明细表。 */
export function CardBusinessDrillTable({
    data,
    columns,
    pagination,
    tableSorting,
    onPaginationChange,
    onSortingChange,
    onClearFilters,
}: CardBusinessDrillTableProps) {
    if (data.rows.total === 0) {
        return (
            <BusinessEmptyState
                kind="filter"
                title="当前筛选无卡券经营记录"
                description="请调整期间、客户、销售单、成本口径或覆盖筛选。"
                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                action={
                    <Button
                        id="card-contracts-analytics-drill-table-clear"
                        type="button"
                        size="sm"
                        variant="secondary"
                        className="rounded-lg shadow-none"
                        onClick={onClearFilters}
                    >
                        清除筛选
                    </Button>
                }
            />
        )
    }
    return (
        <BusinessTableFrame
            title="下钻明细"
            description={`客户 / 销售单 / 卡券类目 / 卡实例引用 / 消费(含税) / 退款(含税) / 成本口径 / 成本(不含税) / 覆盖 / 未履约余额(含税) / 下钻。不包含卡号、卡密与绑定手机号。共 ${data.rows.total} 行 · ${data.filterSummary}`}
            table={
                <DataTable
                    id="card-contracts-analytics-drill-table"
                    columns={columns}
                    data={[...data.rows.items]}
                    getRowId={(row) => row.rowId}
                    rowCount={data.rows.total}
                    pagination={pagination}
                    onPaginationChange={onPaginationChange}
                    sorting={tableSorting}
                    onSortingChange={onSortingChange}
                    manualPagination
                    layout="flush"
                />
            }
        />
    )
}
