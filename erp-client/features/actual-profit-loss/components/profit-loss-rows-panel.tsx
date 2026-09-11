"use client"

import * as React from "react"
import type {
    ColumnDef,
    PaginationState,
    SortingState,
} from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DataTable,
    FixedOptionRadioFilter,
    MultiOptionCombobox,
    OptionCombobox,
} from "@/components/business"
import {
    ListSearchField,
    ListWorkSurface,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    ListWorkspaceViews,
    listWorkspaceEmptyStateClassName,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import type { ComboboxOption } from "@/components/business/option-combobox"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { COST_TYPE_CHIP_PREFIX } from "@/features/actual-profit-loss/hooks/profit-loss-filter-contract"
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
import { toAutomationIdSegment } from "@/lib/automation-id"

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
    appliedChips: readonly ProfitLossAppliedChip[]
    onRemoveFilter: (key: string) => void
    onResetMoreFilters: () => void
    onClearAllFilters: () => void
    hasPendingChanges: boolean
    onDimensionChange: (value: string) => void
    benefitScenarioDraft: string
    onBenefitScenarioDraftChange: SetState<string>
    costTypesDraft: readonly string[]
    onCostTypesDraftChange: (value: string[]) => void
    benefitScenarioOptions: readonly ComboboxOption[]
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
 * 盈亏明细区：ListWorkspaceFilterBar + 维度切换 + 失败/空态/数据表。
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
    appliedChips,
    onRemoveFilter,
    onResetMoreFilters,
    onClearAllFilters,
    hasPendingChanges,
    onDimensionChange,
    benefitScenarioDraft,
    onBenefitScenarioDraftChange,
    costTypesDraft,
    onCostTypesDraftChange,
    benefitScenarioOptions,
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
    const moreCount = appliedChips.filter(
        ({ key }) =>
            key === "benefitScenario" || key.startsWith(COST_TYPE_CHIP_PREFIX),
    ).length
    const listLoadFailed = isError && !data

    return (
        <ListWorkSurface
            ariaLabel={`实际经营盈亏明细 · ${SCOPE_LABEL}`}
            views={
                <ListWorkspaceViews
                    ariaLabel="盈亏明细维度"
                    hint={
                        data
                            ? "明细与指标、汇总同一数据范围 · 点击盈亏下钻销售单 · 点击成本金额打开成本记录详情"
                            : undefined
                    }
                    items={(
                        Object.keys(DIMENSION_LABEL) as ProfitLossDimension[]
                    ).map((key) => ({
                        id: `actual-profit-loss-dimension-${toAutomationIdSegment(key)}`,
                        label: DIMENSION_LABEL[key],
                        count:
                            key === dimension
                                ? data
                                    ? data.rows.total.toLocaleString("zh-CN")
                                    : 0
                                : undefined,
                        active: dimension === key,
                        onClick: () => onDimensionChange(key),
                    }))}
                />
            }
            toolbar={
                <ListWorkspaceFilterBar
                    idPrefix="actual-profit-loss-filter"
                    formAriaLabel="盈亏明细查询"
                    onSubmit={onApplyFilters}
                    queryButtonId="actual-profit-loss-filter-apply"
                    moreButtonId="actual-profit-loss-filter-more-trigger"
                    resetMoreButtonId="actual-profit-loss-filter-reset"
                    clearButtonId="actual-profit-loss-filter-clear-all"
                    search={
                        <ListSearchField
                            id="actual-profit-loss-filter-search"
                            searchInputRef={searchInputRef}
                            value={searchInput}
                            onChange={onSearchInputChange}
                            placeholder="搜索销售单号、客户（/）"
                            aria-label="搜索销售单或客户"
                        />
                    }
                    moreCount={moreCount}
                    moreOpen={panelOpen}
                    onToggleMore={() => setPanelOpen((open) => !open)}
                    morePanelId="actual-profit-loss-filter-more-panel"
                    morePanelAriaLabel="盈亏明细更多筛选条件"
                    onResetMore={onResetMoreFilters}
                    commonFilters={
                        <FixedOptionRadioFilter
                            idPrefix="actual-profit-loss-coverage"
                            label="成本覆盖"
                            variant="quiet"
                            value={coverage}
                            onValueChange={onCoverageChange}
                            options={COVERAGE_OPTIONS}
                        />
                    }
                    morePanel={
                        <div className="grid min-w-0 gap-5 sm:grid-cols-2 lg:grid-cols-3">
                            <ListWorkspaceFilterField
                                htmlFor="actual-profit-loss-filter-benefit-scenario"
                                label="福利场景"
                            >
                                <OptionCombobox
                                    id="actual-profit-loss-filter-benefit-scenario"
                                    className="w-full"
                                    value={benefitScenarioDraft || undefined}
                                    aria-label="福利场景"
                                    onValueChange={(value) =>
                                        onBenefitScenarioDraftChange(
                                            value ?? "",
                                        )
                                    }
                                    options={benefitScenarioOptions}
                                    placeholder="全部福利场景"
                                    searchPlaceholder="搜索福利场景"
                                />
                            </ListWorkspaceFilterField>
                            <ListWorkspaceFilterField
                                htmlFor="actual-profit-loss-filter-cost-types"
                                label="成本类型"
                            >
                                <MultiOptionCombobox
                                    id="actual-profit-loss-filter-cost-types"
                                    className="w-full"
                                    value={costTypesDraft}
                                    aria-label="成本类型"
                                    onValueChange={onCostTypesDraftChange}
                                    options={costTypeOptions}
                                    placeholder="全部成本类型"
                                />
                            </ListWorkspaceFilterField>
                        </div>
                    }
                    resultStatus={listWorkspaceFilterStatusText({
                        loading: loading || (!data && !isError),
                        failed: isError,
                        resultCount: data?.rows.total,
                        noun: "条明细",
                        loadingLabel: "正在加载盈亏明细…",
                    })}
                    chips={appliedChips}
                    onClearChip={onRemoveFilter}
                    onClearAll={onClearAllFilters}
                    hasPendingChanges={hasPendingChanges}
                    pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
                    idleHint="导出全部匹配行，采用生成时的最新数据"
                />
            }
            table={
                listLoadFailed ? (
                    <div className="p-4">
                        <BusinessFailureState
                            title="盈亏数据加载失败"
                            error={error}
                            action={
                                <Button
                                    id="actual-profit-loss-table-retry"
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                    onClick={onRetry}
                                >
                                    重试
                                </Button>
                            }
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
                        className={listWorkspaceEmptyStateClassName}
                        action={
                            hasFilters ? (
                                <Button
                                    id="actual-profit-loss-empty-clear-filters"
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
                        id="actual-profit-loss-table"
                        data={pageRows}
                        columns={columns}
                        defaultColumnVisibility={{
                            benefitScenarios: false,
                            fulfillmentModes: false,
                            latestCostOccurredAt: false,
                        }}
                        defaultColumnPinning={{ left: ["identityLabel"] }}
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
