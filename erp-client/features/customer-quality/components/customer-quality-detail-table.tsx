"use client"

import * as React from "react"
import type { ColumnDef, PaginationState, SortingState } from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessTableFrame,
    DataTable,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type { CustomerQualityRow, CustomerQualityView } from "../types"

export function CustomerQualityDetailTable({
    sectionRef,
    items,
    filteredTotal,
    columns,
    pagination,
    onPaginationChange,
    sorting,
    onSortingChange,
    emptyKind,
    filterSummary,
    onClearFilters,
}: {
    sectionRef: React.RefObject<HTMLDivElement | null>
    items: readonly CustomerQualityRow[]
    filteredTotal: number
    columns: ColumnDef<CustomerQualityRow>[]
    pagination: PaginationState
    onPaginationChange: (next: PaginationState) => void
    sorting: SortingState
    onSortingChange: (next: SortingState) => void
    emptyKind?: CustomerQualityView["emptyKind"]
    filterSummary: string
    onClearFilters: () => void
}) {
    if (emptyKind === "no-data") {
        return (
            <BusinessEmptyState
                kind="no-data"
                title="期间内无授权经营记录"
                description="可调整统计期间或数据范围后重查。"
                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
            />
        )
    }
    if (emptyKind === "filter") {
        return (
            <BusinessEmptyState
                kind="filter"
                title="当前筛选无客户结果"
                description={
                    <>
                        总体指标仍按当前期间与权限范围聚合（未因表格筛选归零）。筛选：
                        {filterSummary}
                    </>
                }
                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                action={
                    <Button
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
        <div
            ref={sectionRef}
            tabIndex={-1}
            data-slot="customer-detail-table"
            className="focus:outline-none"
        >
            <BusinessTableFrame
                title="客户明细"
                description="点击客户进入客户中心；逾期与实际盈亏可分别下钻。金额口径与指标、图表、导出一致。"
                table={
                    <DataTable
                        data={[...items]}
                        columns={columns}
                        getRowId={(row) => row.customerId}
                        rowCount={filteredTotal}
                        pagination={pagination}
                        onPaginationChange={onPaginationChange}
                        sorting={sorting}
                        onSortingChange={onSortingChange}
                        layout="flush"
                        density="compact"
                    />
                }
            />
        </div>
    )
}
