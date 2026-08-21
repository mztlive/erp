"use client"

import * as React from "react"
import {
    ChevronDownIcon,
    FilterIcon,
    SearchIcon,
} from "lucide-react"
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
    FilterChip,
    ListToolbar,
    MultiOptionCombobox,
    OptionCombobox,
} from "@/components/business"
import type { ComboboxOption } from "@/components/business/option-combobox"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { Skeleton } from "@/components/ui/skeleton"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import type { ProfitLossAppliedChip } from "@/features/actual-profit-loss/hooks/use-actual-profit-loss-page"
import { PROFIT_LOSS_SCOPE_LABEL as SCOPE_LABEL } from "@/features/actual-profit-loss/lib/presentation"
import {
    COVERAGE_FILTER_LABEL,
    DIMENSION_LABEL,
    type ProfitLossCoverage,
    type ProfitLossDimension,
    type ProfitLossRow,
    type ProfitLossView,
} from "@/features/actual-profit-loss/types"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

const COVERAGE_OPTIONS: ReadonlyArray<{
    value: ProfitLossCoverage
    label: string
}> = [
    { value: "covered", label: COVERAGE_FILTER_LABEL.covered },
    { value: "uncovered", label: COVERAGE_FILTER_LABEL.uncovered },
    { value: "all", label: COVERAGE_FILTER_LABEL.all },
]

export type ProfitLossRowsPanelProps = {
    /** 查询成功后的视图；加载/失败时为 undefined。 */
    data?: ProfitLossView
    dimension: ProfitLossDimension
    coverage: ProfitLossCoverage
    hasFilters: boolean
    searchInput: string
    searchInputRef: React.RefObject<HTMLInputElement | null>
    onSearchInputChange: (value: string) => void
    onApplyFilters: () => void
    onCoverageChange: (value: string) => void
    panelOpen: boolean
    setPanelOpen: SetState<boolean>
    hasStructuredFilters: boolean
    appliedChips: readonly ProfitLossAppliedChip[]
    onRemoveFilter: (key: string) => void
    onResetMoreFilters: () => void
    onClearAllFilters: () => void
    onDimensionChange: (value: string) => void
    benefitScenarioDraft: string
    onBenefitScenarioDraftChange: SetState<string>
    fulfillmentModesDraft: readonly string[]
    onFulfillmentModesDraftChange: (value: string[]) => void
    costTypesDraft: readonly string[]
    onCostTypesDraftChange: (value: string[]) => void
    benefitScenarioOptions: readonly ComboboxOption[]
    fulfillmentModeOptions: readonly ComboboxOption[]
    costTypeOptions: readonly ComboboxOption[]
    pageRows: ProfitLossRow[]
    columns: ColumnDef<ProfitLossRow>[]
    pagination: PaginationState
    onPaginationChange: (state: PaginationState) => void
    sorting: SortingState
    onSortingChange: (state: SortingState) => void
    loading: boolean
    isError: boolean
    error: unknown
    onRetry: () => void
}

/**
 * 盈亏明细区：筛选 form（ListToolbar + 更多筛选面板 + 已筛选 chip 行）+ 维度切换
 * + 失败/空态/数据表。结构契约见 docs/ui-filter-design.md §1.2 / §3 / §8.2。
 */
export function ProfitLossRowsPanel({
    data,
    dimension,
    coverage,
    hasFilters,
    searchInput,
    searchInputRef,
    onSearchInputChange,
    onApplyFilters,
    onCoverageChange,
    panelOpen,
    setPanelOpen,
    hasStructuredFilters,
    appliedChips,
    onRemoveFilter,
    onResetMoreFilters,
    onClearAllFilters,
    onDimensionChange,
    benefitScenarioDraft,
    onBenefitScenarioDraftChange,
    fulfillmentModesDraft,
    onFulfillmentModesDraftChange,
    costTypesDraft,
    onCostTypesDraftChange,
    benefitScenarioOptions,
    fulfillmentModeOptions,
    costTypeOptions,
    pageRows,
    columns,
    pagination,
    onPaginationChange,
    sorting,
    onSortingChange,
    loading,
    isError,
    error,
    onRetry,
}: ProfitLossRowsPanelProps) {
    const panelId = React.useId()
    const hasChips = appliedChips.length > 0
    const listLoadFailed = isError && !data

    return (
        <BusinessTableFrame
            showHeader
            title={
                <span className="inline-flex items-baseline gap-2">
                    {`明细 · ${DIMENSION_LABEL[dimension]}（${SCOPE_LABEL}）`}
                    <span
                        aria-live="polite"
                        className="font-normal text-muted-foreground"
                    >
                        {data ? data.rows.total.toLocaleString("zh-CN") : 0} 条
                    </span>
                </span>
            }
            description={
                data
                    ? "明细与指标、汇总同一数据范围（趋势与构成图为固定口径序列）· 点击盈亏下钻销售单 · 点击成本金额打开成本记录详情"
                    : undefined
            }
            toolbar={
                <div className="space-y-2">
                    <form
                        onSubmit={(event) => {
                            event.preventDefault()
                            onApplyFilters()
                        }}
                    >
                        <ListToolbar
                            aria-label="盈亏明细筛选"
                            search={
                                <InputGroup>
                                    <InputGroupAddon>
                                        <SearchIcon aria-hidden="true" />
                                    </InputGroupAddon>
                                    <InputGroupInput
                                        ref={searchInputRef}
                                        value={searchInput}
                                        onChange={(event) =>
                                            onSearchInputChange(
                                                event.target.value,
                                            )
                                        }
                                        placeholder="搜索销售单号、客户（/）"
                                        aria-label="搜索销售单或客户"
                                    />
                                    
                                </InputGroup>
                            }
                            filters={
                                <>
                                    <div
                                        role="group"
                                        aria-label="成本覆盖快捷筛选"
                                        className="flex h-control max-w-full items-stretch overflow-x-auto rounded-lg border bg-muted/40 p-0.5 [&_[data-slot=button]]:h-full [&_[data-slot=button]]:min-h-0"
                                    >
                                        {COVERAGE_OPTIONS.map((option) => {
                                            const active =
                                                coverage === option.value
                                            return (
                                                <Button
                                                    key={option.value}
                                                    type="button"
                                                    variant={
                                                        active
                                                            ? "secondary"
                                                            : "ghost"
                                                    }
                                                    className={
                                                        active
                                                            ? "bg-card shadow-xs"
                                                            : "shadow-none"
                                                    }
                                                    aria-pressed={active}
                                                    onClick={() =>
                                                        onCoverageChange(
                                                            option.value,
                                                        )
                                                    }
                                                >
                                                    {option.label}
                                                </Button>
                                            )
                                        })}
                                    </div>
                                    <Button
                                        type="button"
                                        variant="outline"
                                        aria-expanded={panelOpen}
                                        aria-controls={panelId}
                                        onClick={() =>
                                            setPanelOpen((open) => !open)
                                        }
                                    >
                                        <FilterIcon
                                            data-icon="inline-start"
                                            aria-hidden="true"
                                        />
                                        更多筛选
                                        {hasStructuredFilters ? (
                                            <Badge variant="info">
                                                已启用
                                            </Badge>
                                        ) : null}
                                        <ChevronDownIcon
                                            data-icon="inline-end"
                                            aria-hidden="true"
                                            className={
                                                panelOpen
                                                    ? "rotate-180 transition-transform"
                                                    : "transition-transform"
                                            }
                                        />
                                    </Button>
                                </>
                            }
                            secondary={
                                hasChips || panelOpen ? (
                                    <div className="w-full space-y-3">
                                        {hasChips ? (
                                            <div className="flex flex-wrap items-center gap-2 border-t pt-3">
                                                <span className="text-xs text-muted-foreground">
                                                    已筛选
                                                </span>
                                                {appliedChips.map((chip) => (
                                                    <FilterChip
                                                        key={chip.key}
                                                        label={chip.label}
                                                        clearLabel={`移除${chip.label}`}
                                                        onClear={() =>
                                                            onRemoveFilter(
                                                                chip.key,
                                                            )
                                                        }
                                                    />
                                                ))}
                                                <Button
                                                    type="button"
                                                    variant="ghost"
                                                    size="xs"
                                                    onClick={onClearAllFilters}
                                                >
                                                    清空全部
                                                </Button>
                                            </div>
                                        ) : null}
                                        {panelOpen ? (
                                            <div
                                                id={panelId}
                                                className="flex w-full flex-col gap-3 border-t pt-3"
                                                aria-label="盈亏明细更多筛选条件"
                                            >
                                                <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
                                                    <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                                        <span className="text-muted-foreground">
                                                            福利场景
                                                        </span>
                                                        <OptionCombobox
                                                            className="w-full"
                                                            value={
                                                                benefitScenarioDraft ||
                                                                undefined
                                                            }
                                                            aria-label="福利场景"
                                                            onValueChange={(
                                                                value,
                                                            ) =>
                                                                onBenefitScenarioDraftChange(
                                                                    value ?? "",
                                                                )
                                                            }
                                                            options={
                                                                benefitScenarioOptions
                                                            }
                                                            placeholder="全部福利场景"
                                                            searchPlaceholder="搜索福利场景"
                                                        />
                                                    </div>
                                                    <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                                        <span className="text-muted-foreground">
                                                            履约方式
                                                        </span>
                                                        <MultiOptionCombobox
                                                            className="w-full"
                                                            value={
                                                                fulfillmentModesDraft
                                                            }
                                                            aria-label="履约方式"
                                                            onValueChange={
                                                                onFulfillmentModesDraftChange
                                                            }
                                                            options={
                                                                fulfillmentModeOptions
                                                            }
                                                            placeholder="全部履约方式"
                                                        />
                                                    </div>
                                                    <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                                        <span className="text-muted-foreground">
                                                            成本类型
                                                        </span>
                                                        <MultiOptionCombobox
                                                            className="w-full"
                                                            value={
                                                                costTypesDraft
                                                            }
                                                            aria-label="成本类型"
                                                            onValueChange={
                                                                onCostTypesDraftChange
                                                            }
                                                            options={
                                                                costTypeOptions
                                                            }
                                                            placeholder="全部成本类型"
                                                        />
                                                    </div>
                                                </div>
                                                <div className="flex flex-col gap-3 border-t pt-3 sm:flex-row sm:items-center sm:justify-between">
                                                    <p className="text-xs text-muted-foreground">
                                                        将同时应用上方关键词和以下筛选条件；结果也用于导出。
                                                    </p>
                                                    <div className="flex flex-wrap items-center gap-2 sm:justify-end">
                                                        <Button
                                                            type="button"
                                                            variant="ghost"
                                                            onClick={
                                                                onResetMoreFilters
                                                            }
                                                        >
                                                            重置更多条件
                                                        </Button>
                                                        <Button type="submit">
                                                            <SearchIcon
                                                                data-icon="inline-start"
                                                                aria-hidden="true"
                                                            />
                                                            应用全部筛选
                                                        </Button>
                                                    </div>
                                                </div>
                                            </div>
                                        ) : null}
                                    </div>
                                ) : undefined
                            }
                        />
                    </form>
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
                listLoadFailed ? (
                    <div className="p-4">
                        <BusinessFailureState
                            title="盈亏数据加载失败"
                            error={error}
                            onRetry={onRetry}
                        />
                    </div>
                ) : !data ? (
                    <div className="flex flex-col gap-2 p-4">
                        <Skeleton className="h-10 w-full rounded-md" />
                        <Skeleton className="h-10 w-full rounded-md" />
                        <Skeleton className="h-10 w-2/3 rounded-md" />
                    </div>
                ) : data.rows.total === 0 ? (
                    <BusinessEmptyState
                        kind={hasFilters ? "filter" : "no-data"}
                        title={
                            hasFilters ? "当前筛选无结果" : "期间内没有经营结果"
                        }
                        description={
                            hasFilters
                                ? "没有记录符合当前筛选条件，可清除筛选后重试。"
                                : "可调整统计期间或覆盖口径后重试。"
                        }
                        className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                        action={
                            hasFilters ? (
                                <Button
                                    type="button"
                                    variant="secondary"
                                    size="sm"
                                    className="rounded-lg shadow-none"
                                    onClick={onClearAllFilters}
                                >
                                    清除筛选
                                </Button>
                            ) : undefined
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
                    />
                )
            }
        />
    )
}
