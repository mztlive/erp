import * as React from "react"
import { SearchIcon } from "lucide-react"
import type {
    ColumnDef,
    PaginationState,
    SortingState,
} from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessTableFrame,
    DataTable,
    FilterChip,
    ListToolbar,
    OptionCombobox,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { Label } from "@/components/ui/label"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import {
    COVERAGE_FILTER_LABEL,
    DIMENSION_LABEL,
    type ProfitLossCoverage,
    type ProfitLossDimension,
    type ProfitLossRow,
    type ProfitLossView,
} from "@/features/actual-profit-loss/types"
import { PROFIT_LOSS_SCOPE_LABEL as SCOPE_LABEL } from "@/features/actual-profit-loss/lib/presentation"

export function ProfitLossRowsPanel({
    data,
    dimension,
    coverage,
    hasFilters,
    searchInput,
    searchInputRef,
    onSearchInputChange,
    onSearchCommit,
    onCoverageChange,
    customerId,
    salesOrderId,
    onClearCustomer,
    onClearSalesOrder,
    onClearFilters,
    onDimensionChange,
    pageRows,
    columns,
    pagination,
    onPaginationChange,
    sorting,
    onSortingChange,
    loading,
}: {
    data: ProfitLossView
    dimension: ProfitLossDimension
    coverage: ProfitLossCoverage
    hasFilters: boolean
    searchInput: string
    searchInputRef: React.RefObject<HTMLInputElement | null>
    onSearchInputChange: (value: string) => void
    onSearchCommit: () => void
    onCoverageChange: (value: string) => void
    customerId?: string
    salesOrderId?: string
    onClearCustomer: () => void
    onClearSalesOrder: () => void
    onClearFilters: () => void
    onDimensionChange: (value: string) => void
    pageRows: ProfitLossRow[]
    columns: ColumnDef<ProfitLossRow>[]
    pagination: PaginationState
    onPaginationChange: (state: PaginationState) => void
    sorting: SortingState
    onSortingChange: (state: SortingState) => void
    loading: boolean
}) {
    return (
        <BusinessTableFrame
            title={`明细 · ${DIMENSION_LABEL[dimension]}（${SCOPE_LABEL}）`}
            description={`共 ${data.rows.total} 行 · 明细与指标/汇总同一数据范围（趋势与构成图为固定口径序列）· 点击盈亏下钻销售单 · 点击成本金额打开成本记录详情`}
            toolbar={
                <div className="space-y-2">
                    <ListToolbar
                        aria-label="盈亏明细筛选"
                        search={
                            <InputGroup className="max-w-sm">
                                <InputGroupAddon>
                                    <SearchIcon className="size-4" />
                                </InputGroupAddon>
                                <InputGroupInput
                                    ref={searchInputRef}
                                    placeholder="搜索销售单号、客户（/）"
                                    value={searchInput}
                                    onChange={(e) =>
                                        onSearchInputChange(e.target.value)
                                    }
                                    onKeyDown={(e) => {
                                        if (e.key === "Enter") {
                                            onSearchCommit()
                                        }
                                    }}
                                    aria-label="搜索销售单或客户"
                                />
                            </InputGroup>
                        }
                        filters={
                            <>
                                <Label
                                    htmlFor="coverage-filter"
                                    className="sr-only"
                                >
                                    成本覆盖
                                </Label>
                                <OptionCombobox
                                    id="coverage-filter"
                                    value={coverage}
                                    onValueChange={(v) =>
                                        onCoverageChange(v ?? coverage)
                                    }
                                    options={(
                                        Object.keys(
                                            COVERAGE_FILTER_LABEL,
                                        ) as ProfitLossCoverage[]
                                    ).map((key) => ({
                                        value: key,
                                        label: COVERAGE_FILTER_LABEL[key],
                                    }))}
                                    className="w-[10rem]"
                                    size="sm"
                                    allowClear={false}
                                    aria-label="成本覆盖"
                                    placeholder="成本覆盖"
                                />
                            </>
                        }
                        secondary={
                            customerId || salesOrderId ? (
                                <>
                                    {customerId ? (
                                        <FilterChip
                                            label="客户锁定"
                                            onClear={() => {
                                                onClearCustomer()
                                            }}
                                            clearLabel="移除客户锁定"
                                        />
                                    ) : null}
                                    {salesOrderId ? (
                                        <FilterChip
                                            label="销售单锁定"
                                            onClear={() => {
                                                onClearSalesOrder()
                                            }}
                                            clearLabel="移除销售单锁定"
                                        />
                                    ) : null}
                                </>
                            ) : undefined
                        }
                        actions={
                            <div className="flex items-center gap-2 text-xs text-muted-foreground">
                                <span aria-live="polite">
                                    共{" "}
                                    {data.rows.total.toLocaleString("zh-CN")}{" "}
                                    条
                                </span>
                                {hasFilters ? (
                                    <Button
                                        type="button"
                                        size="xs"
                                        variant="ghost"
                                        onClick={onClearFilters}
                                    >
                                        清除筛选
                                    </Button>
                                ) : null}
                            </div>
                        }
                    />
                    <Tabs
                        value={dimension}
                        onValueChange={(v) => {
                            onDimensionChange(v)
                        }}
                    >
                        <TabsList>
                            {(
                                Object.keys(
                                    DIMENSION_LABEL,
                                ) as ProfitLossDimension[]
                            ).map((key) => (
                                <TabsTrigger key={key} value={key}>
                                    {DIMENSION_LABEL[key]}
                                </TabsTrigger>
                            ))}
                        </TabsList>
                    </Tabs>
                </div>
            }
            table={
                data.rows.total === 0 ? (
                    <BusinessEmptyState
                        kind="filter"
                        title="当前筛选无非卡券经营结果"
                        description={`范围：${data.filterSummary}。可调整期间、覆盖口径或清除搜索。`}
                        className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                        action={
                            <Button
                                type="button"
                                variant="secondary"
                                className="rounded-lg shadow-none"
                                onClick={onClearFilters}
                            >
                                清除筛选
                            </Button>
                        }
                    />
                ) : (
                    <DataTable
                        data={pageRows}
                        columns={columns}
                        getRowId={(row) => row.rowId}
                        rowCount={data.rows.total}
                        pagination={pagination}
                        onPaginationChange={onPaginationChange}
                        sorting={sorting}
                        onSortingChange={onSortingChange}
                        loading={loading}
                        layout="flush"
                        density="compact"
                    />
                )
            }
        />
    )
}
