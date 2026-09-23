"use client"

import type { ResponsibleUserOption } from "@/features/entity-selectors/components/responsible-user-filter"
import { ResponsibleUserFilter } from "@/features/entity-selectors/components/responsible-user-filter"

import { FixedOptionRadioFilter, OptionCombobox } from "@/components/business"
import { DateRangePicker } from "@/components/ui/date-picker"
import {
    SALES_ORDER_COMMERCIAL_STATUS_OPTIONS,
    type SalesOrderCommercialStatusFilter,
} from "@/features/sales-orders/lib/filter-orders"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { SalesOrdersListFilterPanel } from "@/features/sales-orders/components/sales-orders-list-filter-panel"
import type { SalesOrdersAppliedChip } from "@/features/sales-orders/hooks/use-sales-orders-list-chips"
import type {
    SalesOrdersListFilterKey,
    useSalesOrdersListFilters,
} from "@/features/sales-orders/hooks/use-sales-orders-list-filters"

const prefix = "sales-orders-list-filter"
const panelId = `${prefix}-panel`
const MORE_CHIP_KEYS: readonly SalesOrdersListFilterKey[] = [
    "origin",
    "reviewStatus",
    "fulfillment",
    "collection",
    "invoice",
    "closeStatus",
    "customerId",
    "contractId",
    "createdBy",
    "orgUnitIds",
]

export function SalesOrdersListFilterBar({
    ownerOptions,
    filters: f,
    chips,
    resultCount,
    loading,
    failed,
}: {
    ownerOptions: readonly ResponsibleUserOption[]
    filters: ReturnType<typeof useSalesOrdersListFilters>
    chips: readonly SalesOrdersAppliedChip[]
    resultCount?: number
    loading: boolean
    failed: boolean
}) {
    const moreCount = chips.filter(({ key }) =>
        MORE_CHIP_KEYS.includes(key),
    ).length

    return (
        <ListWorkspaceFilterBar
            morePresentation="popover"
            className="[&_[data-slot=list-toolbar-search]]:lg:w-72 [&_[data-slot=list-toolbar-filters]]:min-w-0 [&_[data-slot=list-toolbar-filters]]:shrink [&_[data-slot=list-toolbar-filters]]:self-center"
            idPrefix={prefix}
            formAriaLabel="销售单查询"
            onSubmit={f.applyFilters}
            search={
                <ListSearchField
                    id={`${prefix}-search`}
                    data-slot="so-list-search"
                    value={f.searchDraft}
                    onChange={f.setSearchDraft}
                    placeholder="销售单号、客户、合同号"
                    aria-label="搜索销售单"
                />
            }
            moreCount={moreCount}
            moreOpen={f.filterPanelOpen}
            onToggleMore={() =>
                f.filterPanelOpen
                    ? f.cancelMoreFilters()
                    : f.setFilterPanelOpen(true)
            }
            morePanelId={panelId}
            morePanelAriaLabel="销售单更多筛选条件"
            moreButtonId={`${prefix}-more-toggle`}
            queryButtonId={`${prefix}-apply`}
            resetMoreButtonId={`${prefix}-reset`}
            clearButtonId={`${prefix}-clear-all`}
            onResetMore={f.resetMoreFilters}
            primaryFilters={
                <>
                    <div className="w-44 max-w-full">
                        <ResponsibleUserFilter
                            id="sales-orders-list-owner"
                            label="负责销售"
                            hideLabel
                            value={f.filterDraft.ownerUserIds}
                            options={ownerOptions}
                            onChange={(ownerUserIds) =>
                                f.setFilterDraft((draft) => ({
                                    ...draft,
                                    ownerUserIds,
                                }))
                            }
                        />
                    </div>
                    <OptionCombobox
                        id="sales-orders-list-filter-commercial-status"
                        className="w-48 max-w-full"
                        filterLabel="商业状态"
                        aria-label="商业状态"
                        value={
                            f.filterDraft.commercialStatus === "all"
                                ? null
                                : f.filterDraft.commercialStatus
                        }
                        options={SALES_ORDER_COMMERCIAL_STATUS_OPTIONS}
                        onValueChange={(value) =>
                            f.setFilterDraft((draft) => ({
                                ...draft,
                                commercialStatus: (value ??
                                    "all") as SalesOrderCommercialStatusFilter,
                            }))
                        }
                        placeholder="全部"
                    />
                    <DateRangePicker
                        id="sales-orders-list-filter-created-date"
                        className="w-56 max-w-full [&_button]:h-control"
                        value={
                            f.filterDraft.createdFrom || f.filterDraft.createdTo
                                ? {
                                      from:
                                          f.filterDraft.createdFrom ||
                                          undefined,
                                      to: f.filterDraft.createdTo || undefined,
                                  }
                                : undefined
                        }
                        onValueChange={(range) =>
                            f.setFilterDraft((draft) => ({
                                ...draft,
                                createdFrom: range?.from ?? "",
                                createdTo: range?.to ?? "",
                            }))
                        }
                        filterLabel="创建日期"
                        placeholder="全部"
                    />
                </>
            }
            commonFilters={
                <FixedOptionRadioFilter
                    id={`${prefix}-nature`}
                    label="业务性质"
                    variant="quiet"
                    value={f.filterDraft.nature}
                    onValueChange={(nature) => {
                        f.setFilterDraft((draft) => ({
                            ...draft,
                            nature,
                        }))
                    }}
                    options={[
                        { value: "all", label: "全部" },
                        {
                            value: "physical_service",
                            label: "实物与服务",
                        },
                        {
                            value: "card_voucher",
                            label: "卡券",
                        },
                    ]}
                />
            }
            morePanel={
                <SalesOrdersListFilterPanel
                    draft={f.filterDraft}
                    onDraftChange={f.setFilterDraft}
                />
            }
            resultStatus={listWorkspaceFilterStatusText({
                loading,
                failed,
                resultCount,
                noun: "张销售单",
                loadingLabel: "正在加载销售单…",
            })}
            chips={chips}
            onClearChip={(key) =>
                f.removeFilter(key as SalesOrdersListFilterKey)
            }
            onClearAll={f.clearFilters}
            hasPendingChanges={f.hasPendingChanges}
            pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
            idleHint="导出与当前查询结果一致"
        />
    )
}
