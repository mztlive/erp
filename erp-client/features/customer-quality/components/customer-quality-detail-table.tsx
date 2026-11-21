"use client"

import * as React from "react"
import type {
    ColumnDef,
    PaginationState,
    SortingState,
} from "@tanstack/react-table"

import { BusinessFailureState } from "@/components/business"
import {
    ListDataTable,
    ListWorkSurface,
} from "@/components/business/list-workspace"
import type { CustomerQualityRow, CustomerQualityView } from "../types"

/**
 * 客户明细结果区：筛选工具栏常驻，空态与错误态只替换表格内容。
 * 筛选无结果时复用工具栏的 clearAllFilters。
 */
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
    toolbar,
    viewError,
    onRetryView,
    loading,
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
    toolbar: React.ReactNode
    viewError: unknown
    onRetryView: () => void
    loading: boolean
}) {
    return (
        <ListWorkSurface
            ariaLabel="客户经营质量明细"
            className="flex-none"
            toolbar={
                <>
                    <div
                        id="customers-quality-detail-heading"
                        ref={sectionRef}
                        tabIndex={-1}
                        data-slot="customer-detail-table"
                        className="relative h-0 focus:outline-none"
                    >
                        <h2 className="sr-only">客户经营质量明细</h2>
                    </div>
                    {toolbar}
                </>
            }
            tableClassName="[&_[data-slot=table-container]]:max-h-[max(16rem,60dvh)] [&_[data-slot=table-container]]:overflow-auto"
            table={
                <ListDataTable
                    id="customers-quality-detail-table"
                    caption="客户经营质量明细"
                    data={[...items]}
                    columns={columns}
                    defaultColumnOrder={[
                        "customerName",
                        "tags",
                        "salesGrossAmount",
                        "costCoverageRate",
                        "actualProfitLossNet",
                        "receivableOpenGross",
                        "exceptions",
                        "latestBusinessAt",
                    ]}
                    defaultColumnVisibility={{ latestBusinessAt: false }}
                    defaultColumnPinning={{ left: ["customerName"] }}
                    getRowId={(row) => row.customerId}
                    rowLabel={(row) =>
                        `${row.customerName}（${row.customerNo}）`
                    }
                    rowCount={filteredTotal}
                    pagination={pagination}
                    onPaginationChange={onPaginationChange}
                    sorting={sorting}
                    onSortingChange={onSortingChange}
                    loading={loading}
                    showToolbarSummary={false}
                    hasActiveFilters={emptyKind === "filter"}
                    clearFiltersButtonId="customers-quality-detail-clear-filters"
                    onClearFilters={onClearFilters}
                    emptyCopy={{
                        filterTitle: "当前筛选无客户结果",
                        filterDescription: `上方汇总指标统计当前期间与权限范围内的全部客户。筛选：${filterSummary}`,
                        emptyTitle: "期间内无授权经营记录",
                        emptyDescription: "可调整统计期间或数据范围后重查。",
                    }}
                    errorState={
                        viewError != null ? (
                            <BusinessFailureState
                                idPrefix="customers-quality-detail-table"
                                title="经营质量数据加载失败"
                                error={viewError}
                                onRetry={onRetryView}
                            />
                        ) : undefined
                    }
                />
            }
        />
    )
}
