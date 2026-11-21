"use client"

import type * as React from "react"

import { OptionCombobox, MultiOptionCombobox } from "@/components/business"
import {
    ListFilterSection,
    ListFilterSections,
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    listWorkspaceStyles,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { WarehouseSearchCombobox } from "@/features/entity-selectors"
import { PersonDirectoryFilter } from "@/features/entity-selectors/components/person-directory-filter"
import { listFilterText } from "@/lib/ui-text"
import { MOVEMENT_TYPE_OPTIONS } from "@/features/inventory/lib/presentation"
import type {
    LedgerAppliedChip,
    useLedgerFilters,
} from "@/features/inventory/pages/hooks/use-ledger-filters"
import { AVAILABILITY_LABEL } from "@/features/inventory/types"
import type {
    InventoryAvailability,
    InventoryView,
} from "@/features/inventory/types"

const MORE_FILTER_TRIGGER = "inventory-ledger-filters-trigger"

const AVAILABILITY_OPTIONS: ReadonlyArray<{
    value: InventoryAvailability
    label: string
}> = (["all", "positive", "zero", "reserved"] as const).map((value) => ({
    value,
    label: AVAILABILITY_LABEL[value],
}))

type LedgerToolbarProps = ReturnType<typeof useLedgerFilters> & {
    view: InventoryView
    searchInputRef: React.RefObject<HTMLInputElement | null>
    hasActiveFilters: boolean
    appliedChips: readonly LedgerAppliedChip[]
    loading: boolean
    failed: boolean
}

/**
 * 库存台账主行提供仓库与库存条件，查询后统一生效；
 * 流水视图的更多筛选提供流水类型与发生日期。
 */
export function LedgerToolbar({
    canFilterPeople,
    canFilterWarehouse,
    view,
    hasActiveFilters,
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
    operatorUserIdsDraft,
    setOperatorUserIdsDraft,
    applicantUserIdsDraft,
    setApplicantUserIdsDraft,
    handlerUserIdsDraft,
    setHandlerUserIdsDraft,
    panelOpen,
    setPanelOpen,
    appliedChips,
    removeFilter,
    applyFilters,
    resetMoreFilters,
    cancelMoreFilters,
    clearAllFilters,
    filterError,
    setFilterError,
    hasPendingChanges = false,
    loading,
    failed,
}: LedgerToolbarProps) {
    const dateErrorId = "inventory-ledger-occurred-error"
    const showAvailabilityCommon = view === "balance"
    const showMovementMore = view === "movement"
    const showAdjustmentMore = view === "adjustment" && canFilterPeople
    const showMore = showMovementMore || showAdjustmentMore
    const moreCount = appliedChips.filter(({ key }) =>
        showMovementMore
            ? key === "movementType" ||
              key === "occurredRange" ||
              key === "operatorUserIds"
            : showAdjustmentMore &&
              (key === "operatorUserIds" ||
                  key === "applicantUserIds" ||
                  key === "handlerUserIds"),
    ).length

    const warehouseFilter = canFilterWarehouse ? (
        <WarehouseSearchCombobox
            id="inventory-ledger-warehouse-filter"
            className="w-44 max-w-full min-w-0"
            filterLabel="仓库"
            value={warehouseIdDraft ?? undefined}
            onValueChange={(id) => setWarehouseIdDraft(id ?? null)}
            purpose="filter"
            aria-label="仓库"
            placeholder="全部"
        />
    ) : undefined

    return (
        <ListWorkspaceFilterBar
            density="compact"
            morePresentation="popover"
            className="[&_[data-slot=list-toolbar-primary]>div>[data-slot=separator]]:hidden"
            idPrefix="inventory-ledger-filter"
            formAriaLabel="库存台账查询"
            onSubmit={applyFilters}
            search={
                <ListSearchField
                    id="inventory-ledger-search"
                    searchInputRef={searchInputRef}
                    value={searchDraft}
                    onChange={setSearchDraft}
                    placeholder="SKU 编码、名称、规格"
                    aria-label="搜索库存"
                />
            }
            queryButtonId="inventory-ledger-apply-filters"
            moreCount={moreCount}
            moreOpen={showMore && panelOpen}
            onToggleMore={
                showMore
                    ? () => {
                          if (panelOpen) {
                              cancelMoreFilters()
                              document
                                  .getElementById(MORE_FILTER_TRIGGER)
                                  ?.focus()
                          } else {
                              setPanelOpen(true)
                          }
                      }
                    : undefined
            }
            moreButtonId={MORE_FILTER_TRIGGER}
            morePanelId="inventory-ledger-more-panel"
            morePanelAriaLabel="库存台账更多筛选条件"
            onResetMore={showMore ? resetMoreFilters : undefined}
            resetMoreButtonId="inventory-ledger-reset-more"
            actions={
                hasActiveFilters && appliedChips.length === 0 ? (
                    <Button
                        id="inventory-ledger-clear-all"
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={listWorkspaceStyles.quietButton}
                        onClick={clearAllFilters}
                    >
                        {listFilterText.clear}
                    </Button>
                ) : undefined
            }
            primaryFilters={
                <>
                    {warehouseFilter}
                    {showAvailabilityCommon ? (
                        <OptionCombobox
                            id="inventory-ledger-availability-filter"
                            className="w-36 max-w-full min-w-0"
                            filterLabel="库存条件"
                            aria-label="库存条件"
                            value={
                                availabilityDraft === "all"
                                    ? null
                                    : availabilityDraft
                            }
                            onValueChange={(value) =>
                                setAvailabilityDraft(
                                    AVAILABILITY_OPTIONS.find(
                                        (option) => option.value === value,
                                    )?.value ?? "all",
                                )
                            }
                            options={AVAILABILITY_OPTIONS.filter(
                                (option) => option.value !== "all",
                            )}
                            placeholder="全部"
                        />
                    ) : null}
                </>
            }
            morePanel={
                showMore ? (
                    <ListFilterSections>
                        {showMovementMore ? (
                            <ListFilterSection legend="流水">
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
                                <div className="min-w-0 md:col-span-2">
                                    <ListWorkspaceFilterField
                                        htmlFor="inventory-ledger-occurred-from"
                                        label="发生日期"
                                    >
                                        <div
                                            className="flex min-w-0 items-center gap-2"
                                            data-has-value={Boolean(
                                                occurredFromDraft ||
                                                occurredToDraft,
                                            )}
                                        >
                                            <Input
                                                id="inventory-ledger-occurred-from"
                                                type="date"
                                                className="w-0 min-w-0 flex-1"
                                                value={occurredFromDraft}
                                                max={
                                                    occurredToDraft || undefined
                                                }
                                                onChange={(event) => {
                                                    setOccurredFromDraft(
                                                        event.target.value,
                                                    )
                                                    setFilterError(null)
                                                }}
                                                autoComplete="off"
                                                aria-label="发生日期起"
                                                aria-invalid={Boolean(
                                                    filterError,
                                                )}
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
                                                min={
                                                    occurredFromDraft ||
                                                    undefined
                                                }
                                                onChange={(event) => {
                                                    setOccurredToDraft(
                                                        event.target.value,
                                                    )
                                                    setFilterError(null)
                                                }}
                                                autoComplete="off"
                                                aria-label="发生日期止"
                                                aria-invalid={Boolean(
                                                    filterError,
                                                )}
                                                aria-describedby={
                                                    filterError
                                                        ? dateErrorId
                                                        : undefined
                                                }
                                            />
                                        </div>
                                    </ListWorkspaceFilterField>
                                    {filterError ? (
                                        <p
                                            id={dateErrorId}
                                            className="mt-1 text-xs text-destructive"
                                            role="alert"
                                        >
                                            {filterError}
                                        </p>
                                    ) : null}
                                </div>
                            </ListFilterSection>
                        ) : null}
                        {showMovementMore && canFilterPeople ? (
                            <ListFilterSection legend="人员">
                                <PersonDirectoryFilter
                                    id="inventory-ledger-operator-filter"
                                    label="经办人"
                                    value={operatorUserIdsDraft}
                                    onChange={setOperatorUserIdsDraft}
                                    category="business"
                                />
                            </ListFilterSection>
                        ) : null}
                        {showAdjustmentMore ? (
                            <ListFilterSection legend="人员">
                                <PersonDirectoryFilter
                                    id="inventory-ledger-adjustment-operator-filter"
                                    label="经办人"
                                    value={operatorUserIdsDraft}
                                    onChange={setOperatorUserIdsDraft}
                                    category="business"
                                />
                                <PersonDirectoryFilter
                                    id="inventory-ledger-applicant-filter"
                                    label="申请人"
                                    value={applicantUserIdsDraft}
                                    onChange={setApplicantUserIdsDraft}
                                    category="business"
                                />
                                <PersonDirectoryFilter
                                    id="inventory-ledger-handler-filter"
                                    label="当前审批人"
                                    value={handlerUserIdsDraft}
                                    onChange={setHandlerUserIdsDraft}
                                    category="business"
                                />
                            </ListFilterSection>
                        ) : null}
                    </ListFilterSections>
                ) : undefined
            }
            resultStatus={failed ? "查询未完成" : loading ? "查询中…" : null}
            chips={appliedChips.map((chip) => ({
                ...chip,
                onClear: () => removeFilter(chip.key),
            }))}
            onClearAll={clearAllFilters}
            clearButtonId="inventory-ledger-clear-all"
            hasPendingChanges={hasPendingChanges}
            pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
        />
    )
}
