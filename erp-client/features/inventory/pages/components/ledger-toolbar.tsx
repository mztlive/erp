"use client"

import * as React from "react"

import {
    FixedOptionRadioFilter,
    MultiOptionCombobox,
} from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    ListWorkspaceInlineFilter,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { Input } from "@/components/ui/input"
import { WarehouseSearchCombobox } from "@/features/entity-selectors"
import { MOVEMENT_TYPE_OPTIONS } from "@/features/inventory/lib/presentation"
import type {
    LedgerAppliedChip,
    LedgerFilterKey,
} from "@/features/inventory/pages/hooks/use-ledger-filters"
import { AVAILABILITY_LABEL } from "@/features/inventory/types"
import type {
    InventoryAvailability,
    InventoryView,
} from "@/features/inventory/types"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

const AVAILABILITY_RADIO_OPTIONS: ReadonlyArray<{
    value: InventoryAvailability
    label: string
}> = (["all", "positive", "zero", "reserved"] as const).map((value) => ({
    value,
    label: AVAILABILITY_LABEL[value],
}))

interface LedgerToolbarProps {
    view: InventoryView
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: SetState<string>
    warehouseIdDraft: string | null
    setWarehouseIdDraft: SetState<string | null>
    availabilityDraft: InventoryAvailability
    setAvailabilityDraft: SetState<InventoryAvailability>
    movementTypeDraft: string[]
    setMovementTypeDraft: SetState<string[]>
    occurredFromDraft: string
    setOccurredFromDraft: SetState<string>
    occurredToDraft: string
    setOccurredToDraft: SetState<string>
    panelOpen: boolean
    setPanelOpen: SetState<boolean>
    hasStructuredFilters?: boolean
    hasActiveFilters?: boolean
    appliedChips: readonly LedgerAppliedChip[]
    removeFilter: (key: LedgerFilterKey) => void
    applyFilters: () => void
    resetMoreFilters: () => void
    clearAllFilters: () => void
    filterError: string | null
    setFilterError: SetState<string | null>
    hasPendingChanges?: boolean
    resultCount?: number
    loading?: boolean
    failed?: boolean
}

/**
 * 库存台账筛选条：余额视图常用可用状态，其余视图常用仓库；
 * 更多筛选放仓库 / 流水类型 / 发生日期。
 */
export function LedgerToolbar({
    view,
    searchInputRef,
    searchDraft,
    setSearchDraft,
    warehouseIdDraft,
    setWarehouseIdDraft,
    availabilityDraft,
    setAvailabilityDraft,
    movementTypeDraft,
    setMovementTypeDraft,
    occurredFromDraft,
    setOccurredFromDraft,
    occurredToDraft,
    setOccurredToDraft,
    panelOpen,
    setPanelOpen,
    appliedChips,
    removeFilter,
    applyFilters,
    resetMoreFilters,
    clearAllFilters,
    filterError,
    setFilterError,
    hasPendingChanges = false,
    resultCount,
    loading,
    failed,
}: LedgerToolbarProps) {
    const dateErrorId = "inventory-ledger-occurred-error"
    const showAvailabilityCommon = view === "balance"
    const showWarehouseCommon = view !== "balance"
    const showWarehouseMore = view === "balance"
    const showMovementMore = view === "movement"
    const showMore = showWarehouseMore || showMovementMore
    const moreCount = appliedChips.filter(({ key }) => {
        if (key === "warehouseId") return showWarehouseMore
        if (key === "movementType" || key === "occurredRange")
            return showMovementMore
        return (
            key === "skuId" ||
            key === "salesOrderLineId" ||
            key === "adjustmentId"
        )
    }).length

    const warehouseFilter = (
        <WarehouseSearchCombobox
            id="inventory-ledger-warehouse-filter"
            className="w-full sm:w-60"
            value={warehouseIdDraft ?? undefined}
            onValueChange={(id) => setWarehouseIdDraft(id ?? null)}
            purpose="filter"
            aria-label="筛选仓库"
            placeholder="全部仓库"
        />
    )

    return (
        <ListWorkspaceFilterBar
            idPrefix="inventory-ledger-filter"
            formAriaLabel="库存台账查询"
            onSubmit={applyFilters}
            search={
                <ListSearchField
                    id="inventory-ledger-search"
                    searchInputRef={searchInputRef}
                    value={searchDraft}
                    onChange={setSearchDraft}
                    placeholder="SKU 编码、名称、规格、仓库"
                    aria-label="搜索库存"
                />
            }
            queryButtonId="inventory-ledger-apply-filters"
            moreCount={moreCount}
            moreOpen={panelOpen}
            onToggleMore={
                showMore ? () => setPanelOpen((open) => !open) : undefined
            }
            moreButtonId="inventory-ledger-filters-trigger"
            morePanelId="inventory-ledger-more-panel"
            morePanelAriaLabel="库存台账更多筛选条件"
            onResetMore={showMore ? resetMoreFilters : undefined}
            resetMoreButtonId="inventory-ledger-reset-more"
            commonFilters={
                <>
                    {showAvailabilityCommon ? (
                        <FixedOptionRadioFilter
                            id="inventory-ledger-availability-filter"
                            label="可用状态"
                            variant="quiet"
                            value={availabilityDraft}
                            onValueChange={setAvailabilityDraft}
                            options={AVAILABILITY_RADIO_OPTIONS}
                        />
                    ) : null}
                    {showWarehouseCommon ? (
                        <ListWorkspaceInlineFilter
                            htmlFor="inventory-ledger-warehouse-filter"
                            label="仓库"
                        >
                            {warehouseFilter}
                        </ListWorkspaceInlineFilter>
                    ) : null}
                </>
            }
            morePanel={
                showMore ? (
                    <div className="grid min-w-0 gap-5">
                        {showWarehouseMore ? (
                            <ListWorkspaceFilterField
                                htmlFor="inventory-ledger-warehouse-filter"
                                label="仓库"
                            >
                                {warehouseFilter}
                            </ListWorkspaceFilterField>
                        ) : null}
                        {showMovementMore ? (
                            <div className="grid min-w-0 gap-3 sm:grid-cols-2">
                                <ListWorkspaceFilterField
                                    htmlFor="inventory-ledger-movement-type-filter"
                                    label="流水类型"
                                >
                                    <MultiOptionCombobox
                                        id="inventory-ledger-movement-type-filter"
                                        className="w-full"
                                        value={movementTypeDraft}
                                        onValueChange={setMovementTypeDraft}
                                        options={MOVEMENT_TYPE_OPTIONS}
                                        placeholder="全部流水类型"
                                        aria-label="流水类型"
                                    />
                                </ListWorkspaceFilterField>
                                <ListWorkspaceFilterField
                                    label="发生日期"
                                    className="sm:col-span-2"
                                >
                                    <div className="flex min-w-0 items-center gap-2">
                                        <Input
                                            id="inventory-ledger-occurred-from"
                                            type="date"
                                            className="w-0 min-w-0 flex-1"
                                            value={occurredFromDraft}
                                            max={occurredToDraft || undefined}
                                            onChange={(event) => {
                                                setOccurredFromDraft(
                                                    event.target.value,
                                                )
                                                setFilterError(null)
                                            }}
                                            autoComplete="off"
                                            aria-label="发生日期起"
                                            aria-invalid={Boolean(filterError)}
                                            aria-describedby={
                                                filterError
                                                    ? dateErrorId
                                                    : undefined
                                            }
                                        />
                                        <span className="text-xs text-muted-foreground">
                                            至
                                        </span>
                                        <Input
                                            id="inventory-ledger-occurred-to"
                                            type="date"
                                            className="w-0 min-w-0 flex-1"
                                            value={occurredToDraft}
                                            min={occurredFromDraft || undefined}
                                            onChange={(event) => {
                                                setOccurredToDraft(
                                                    event.target.value,
                                                )
                                                setFilterError(null)
                                            }}
                                            autoComplete="off"
                                            aria-label="发生日期止"
                                            aria-invalid={Boolean(filterError)}
                                            aria-describedby={
                                                filterError
                                                    ? dateErrorId
                                                    : undefined
                                            }
                                        />
                                    </div>
                                    {filterError ? (
                                        <p
                                            id={dateErrorId}
                                            className="text-xs text-destructive"
                                            role="alert"
                                        >
                                            {filterError}
                                        </p>
                                    ) : null}
                                </ListWorkspaceFilterField>
                            </div>
                        ) : null}
                    </div>
                ) : undefined
            }
            resultStatus={listWorkspaceFilterStatusText({
                loading,
                failed,
                resultCount,
                noun: "条记录",
                loadingLabel: "正在加载库存…",
            })}
            chips={appliedChips}
            onClearChip={(key) => removeFilter(key as LedgerFilterKey)}
            onClearAll={clearAllFilters}
            clearButtonId="inventory-ledger-clear-all"
            hasPendingChanges={hasPendingChanges}
            pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
            idleHint="导出与当前查询结果一致"
        />
    )
}
