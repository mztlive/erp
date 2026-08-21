"use client"

import * as React from "react"
import type { ColumnDef, PaginationState, SortingState } from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessTableFrame,
    DataTable,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type { CustomerQualityRow, CustomerQualityView } from "../types"

/**
 * 客户明细结果卡（docs/ui-filter-design.md §8 结构）：
 * 筛选 form 经 toolbar 承载在 BusinessTableFrame 内，空态/错误态只替换
 * 表格内容，工具栏常驻；筛选无结果使用与工具栏同一个 clearAllFilters。
 */
export function CustomerQualityDetailTable({
    sectionRef,
    items,
    filteredTotal,
    total,
    columns,
    pagination,
    onPaginationChange,
    sorting,
    onSortingChange,
    emptyKind,
    filterSummary,
    hasActiveFilters,
    onClearFilters,
    toolbar,
    viewError,
    onRetryView,
    loading,
}: {
    sectionRef: React.RefObject<HTMLDivElement | null>
    items: readonly CustomerQualityRow[]
    filteredTotal: number
    total: number
    columns: ColumnDef<CustomerQualityRow>[]
    pagination: PaginationState
    onPaginationChange: (next: PaginationState) => void
    sorting: SortingState
    onSortingChange: (next: SortingState) => void
    emptyKind?: CustomerQualityView["emptyKind"]
    filterSummary: string
    hasActiveFilters: boolean
    onClearFilters: () => void
    toolbar: React.ReactNode
    viewError: unknown
    onRetryView: () => void
    loading: boolean
}) {
    const loadFailed = viewError != null
    const showEmpty = !loadFailed && !loading && items.length === 0

    return (
        <div
            ref={sectionRef}
            tabIndex={-1}
            data-slot="customer-detail-table"
            className="focus:outline-none"
        >
            <BusinessTableFrame
                showHeader
                title={
                    <span className="inline-flex items-baseline gap-2">
                        客户明细
                        <span
                            className="num font-normal text-muted-foreground"
                            aria-live="polite"
                        >
                            {filteredTotal} 户
                        </span>
                    </span>
                }
                description={
                    hasActiveFilters
                        ? `筛选：${filterSummary} · 明细 ${filteredTotal}/${total} 户`
                        : "点击客户进入客户中心；逾期与实际盈亏可分别下钻。金额口径与指标、图表、导出一致。"
                }
                toolbar={toolbar}
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
                        loading={loading}
                        errorState={
                            loadFailed ? (
                                <BusinessFailureState
                                    title="经营质量数据加载失败"
                                    error={viewError}
                                    onRetry={onRetryView}
                                />
                            ) : undefined
                        }
                        emptyState={
                            showEmpty ? (
                                emptyKind === "filter" ? (
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
                                ) : (
                                    <BusinessEmptyState
                                        kind="no-data"
                                        title="期间内无授权经营记录"
                                        description="可调整统计期间或数据范围后重查。"
                                        className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                    />
                                )
                            ) : undefined
                        }
                    />
                }
            />
        </div>
    )
}
