"use client"
import type { HistoricalDirectory } from "@/lib/historical-directory"

import * as React from "react"
import type {
    ColumnDef,
    PaginationState,
    SortingState,
} from "@tanstack/react-table"

import {
    BusinessFailureState,
    MultiOptionCombobox,
    OptionCombobox,
} from "@/components/business"
import {
    ListDataTable,
    ListFilterSection,
    ListFilterSections,
    ListSearchField,
    ListWorkSurface,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    ListWorkspaceViews,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import type { ComboboxOption } from "@/components/business/option-combobox"
import { Button } from "@/components/ui/button"
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

const DEFAULT_COLUMN_VISIBILITY = {
    benefitScenarios: false,
    latestCostOccurredAt: false,
}
const DEFAULT_COLUMN_ORDER = [
    "identityLabel",
    "netSalesRevenue",
    "actualProcurementCostNet",
    "actualFulfillmentCostNet",
    "reductionsNet",
    "actualProfitLossNet",
    "marginRate",
    "coverageState",
    "benefitScenarios",
    "latestCostOccurredAt",
]
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
    historyDirectory?: HistoricalDirectory
    directoryError?: unknown
    onRefreshDirectory: () => Promise<unknown>
    onRetryDirectory?: () => void
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
    onCancelMoreFilters: () => void
    onClearAllFilters: () => void
    hasPendingChanges: boolean
    onDimensionChange: (value: string) => void
    benefitScenarioDraft: string
    onBenefitScenarioDraftChange: SetState<string>
    costTypesDraft: readonly string[]
    onCostTypesDraftChange: (value: string[]) => void
    benefitScenarioOptions: readonly ComboboxOption[]
    attributionUsersDraft: readonly string[]
    onAttributionUsersChange: (values: string[]) => void
    attributionOrgsDraft: readonly string[]
    onAttributionOrgsChange: (values: string[]) => void
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
    historyDirectory,
    directoryError,
    onRetryDirectory,
    onRefreshDirectory,
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
    onCancelMoreFilters,
    onClearAllFilters,
    hasPendingChanges,
    onDimensionChange,
    benefitScenarioDraft,
    onBenefitScenarioDraftChange,
    costTypesDraft,
    onCostTypesDraftChange,
    benefitScenarioOptions,
    costTypeOptions,
    attributionUsersDraft,
    onAttributionUsersChange,
    attributionOrgsDraft,
    onAttributionOrgsChange,
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
            key === "benefitScenario" ||
            key === "attributionUserIds" ||
            key === "attributionOrgUnitIds" ||
            key.startsWith(COST_TYPE_CHIP_PREFIX),
    ).length
    const listLoadFailed = isError && !data
    const rowCount = data?.rows.total ?? 0
    const noScope = data?.emptyReason === "no_scope"

    return (
        <ListWorkSurface
            className="flex-none"
            tableClassName="[&_[data-slot=table-container]]:max-h-[max(16rem,60dvh)]"
            ariaLabel={`实际经营盈亏明细 · ${SCOPE_LABEL}`}
            views={
                <ListWorkspaceViews
                    ariaLabel="盈亏明细维度"
                    hint={
                        data
                            ? "明细与指标使用同一范围 · 点击签单归属查看该组订单 · 点击成本金额可查看明细"
                            : undefined
                    }
                    items={(
                        Object.keys(DIMENSION_LABEL) as ProfitLossDimension[]
                    ).map((key) => ({
                        id: `actual-profit-loss-dimension-${toAutomationIdSegment(key)}`,
                        label: DIMENSION_LABEL[key],
                        count:
                            key === dimension
                                ? rowCount.toLocaleString("zh-CN")
                                : undefined,
                        active: dimension === key,
                        onClick: () => onDimensionChange(key),
                    }))}
                />
            }
            toolbar={
                <ListWorkspaceFilterBar
                    morePresentation="popover"
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
                    onToggleMore={() =>
                        panelOpen ? onCancelMoreFilters() : setPanelOpen(true)
                    }
                    morePanelId="actual-profit-loss-filter-more-panel"
                    morePanelAriaLabel="盈亏明细更多筛选条件"
                    onResetMore={onResetMoreFilters}
                    primaryFilters={
                        <OptionCombobox
                            id="actual-profit-loss-coverage"
                            className="w-60 max-w-full min-w-0"
                            filterLabel="成本归集完整度"
                            aria-label="成本归集完整度"
                            allowClear={false}
                            value={coverage}
                            options={COVERAGE_OPTIONS}
                            onValueChange={(value) => {
                                if (
                                    value === "covered" ||
                                    value === "uncovered" ||
                                    value === "all"
                                ) {
                                    onCoverageChange(value)
                                }
                            }}
                        />
                    }
                    morePanel={
                        <ListFilterSections>
                            {directoryError != null && (
                                <BusinessFailureState
                                    idPrefix="actual-profit-loss-filter-directory"
                                    title="销售与部门列表加载失败"
                                    error={directoryError}
                                    onRetry={onRetryDirectory}
                                />
                            )}
                            <ListFilterSection legend="签单归属">
                                <ListWorkspaceFilterField
                                    htmlFor="actual-profit-loss-filter-attribution-user"
                                    label="签单归属销售"
                                >
                                    <MultiOptionCombobox
                                        onRefresh={onRefreshDirectory}
                                        id="actual-profit-loss-filter-attribution-user"
                                        className="w-full min-w-0"
                                        value={attributionUsersDraft}
                                        onValueChange={onAttributionUsersChange}
                                        options={
                                            historyDirectory?.attributionUserOptions ??
                                            []
                                        }
                                        filterLabel="签单归属销售"
                                        hideFilterLabel
                                        aria-label="签单归属销售"
                                    />
                                </ListWorkspaceFilterField>
                                <ListWorkspaceFilterField
                                    htmlFor="actual-profit-loss-filter-attribution-org"
                                    label="签单所属部门（含当时下级）"
                                >
                                    <MultiOptionCombobox
                                        onRefresh={onRefreshDirectory}
                                        id="actual-profit-loss-filter-attribution-org"
                                        className="w-full min-w-0"
                                        value={attributionOrgsDraft}
                                        onValueChange={onAttributionOrgsChange}
                                        options={
                                            historyDirectory?.attributionOrgOptions ??
                                            []
                                        }
                                        filterLabel="签单所属部门"
                                        hideFilterLabel
                                        aria-label="签单所属部门"
                                    />
                                </ListWorkspaceFilterField>
                            </ListFilterSection>
                            <ListFilterSection legend="场景与成本">
                                <ListWorkspaceFilterField
                                    htmlFor="actual-profit-loss-filter-benefit-scenario"
                                    label="福利场景"
                                >
                                    <OptionCombobox
                                        id="actual-profit-loss-filter-benefit-scenario"
                                        className="w-full min-w-0"
                                        value={
                                            benefitScenarioDraft || undefined
                                        }
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
                                        className="w-full min-w-0"
                                        value={costTypesDraft}
                                        aria-label="成本类型"
                                        onValueChange={onCostTypesDraftChange}
                                        options={costTypeOptions}
                                        filterLabel="成本类型"
                                        hideFilterLabel
                                    />
                                </ListWorkspaceFilterField>
                            </ListFilterSection>
                        </ListFilterSections>
                    }
                    resultStatus={
                        loading || isError
                            ? listWorkspaceFilterStatusText({
                                  loading: loading && !isError,
                                  failed: isError,
                                  noun: "盈亏明细",
                              })
                            : null
                    }
                    chips={appliedChips}
                    onClearChip={onRemoveFilter}
                    onClearAll={onClearAllFilters}
                    hasPendingChanges={hasPendingChanges}
                    pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
                />
            }
            table={
                <ListDataTable
                    id="actual-profit-loss-table"
                    caption="实际经营盈亏明细"
                    data={pageRows}
                    columns={columns}
                    defaultColumnVisibility={DEFAULT_COLUMN_VISIBILITY}
                    defaultColumnOrder={DEFAULT_COLUMN_ORDER}
                    defaultColumnPinning={{ left: ["identityLabel"] }}
                    getRowId={(row) => row.rowId}
                    rowLabel={(row) => row.identityLabel}
                    rowCount={rowCount}
                    pagination={pagination}
                    onPaginationChange={onPaginationChange}
                    sorting={sorting}
                    onSortingChange={rowCount > 0 ? onSortingChange : undefined}
                    showColumnVisibility={rowCount > 0}
                    showPagination={rowCount > 0}
                    showToolbarSummary={false}
                    loading={loading}
                    errorState={
                        listLoadFailed ? (
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
                        ) : undefined
                    }
                    noScope={noScope}
                    hasActiveFilters={hasFilters}
                    onClearFilters={noScope ? undefined : onClearAllFilters}
                    clearFiltersButtonId="actual-profit-loss-empty-clear-filters"
                    emptyCopy={{
                        noScopeTitle: "当前没有可查询的数据范围",
                        noScopeDescription:
                            "账号具备查询权限，但当前授权范围未包含可读取业务，请联系权限管理员核对范围。",
                        emptyTitle: "期间内没有经营结果",
                        emptyDescription:
                            "可调整统计期间或成本归集筛选条件后重试。",
                    }}
                />
            }
        />
    )
}
