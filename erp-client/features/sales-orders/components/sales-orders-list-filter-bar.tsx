"use client"

import type { ResponsibleUserOption } from "@/features/entity-selectors/components/responsible-user-filter"

import { FixedOptionRadioFilter } from "@/components/business"
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
    "commercialStatus",
    "reviewStatus",
    "fulfillment",
    "collection",
    "invoice",
    "closeStatus",
    "customerId",
    "contractId",
    "createdBy",
    "ownerUserIds",
    "createdDate",
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
            onToggleMore={() => f.setFilterPanelOpen((open) => !open)}
            morePanelId={panelId}
            morePanelAriaLabel="销售单更多筛选条件"
            moreButtonId={`${prefix}-more-toggle`}
            queryButtonId={`${prefix}-apply`}
            resetMoreButtonId={`${prefix}-reset`}
            clearButtonId={`${prefix}-clear-all`}
            onResetMore={f.resetMoreFilters}
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
                    ownerOptions={ownerOptions}
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
