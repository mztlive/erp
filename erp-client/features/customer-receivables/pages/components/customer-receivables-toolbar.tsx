"use client"

import * as React from "react"

import { OptionCombobox } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    listWorkspaceStyles,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { ReceivableCounterpartySearchCombobox } from "@/features/customer-receivables/components/receivable-counterparty-search-combobox"
import {
    RECEIVABLE_STATUS_LABEL,
    type CustomerAccountsView,
    type CustomerReceivablesFilterKey,
    type ReceivableStatusFilter,
} from "@/features/customer-receivables/types"
import { listFilterText } from "@/lib/ui-text"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

export type ReceivableAppliedChip = Readonly<{
    key: CustomerReceivablesFilterKey
    label: string
}>

const prefix = "customer-receivables-toolbar"
const FOLDED_CHIP_KEYS = [
    "customerId",
    "due",
    "salesOrderId",
    "receivableAccountId",
]
const NON_RECEIVABLE_CHIP_KEYS = [...FOLDED_CHIP_KEYS, "status"]

const STATUS_OPTIONS = (["open", "partial", "settled"] as const).map(
    (value) => ({
        value,
        label: RECEIVABLE_STATUS_LABEL[value],
    }),
)

type CustomerReceivablesToolbarProps = {
    view: CustomerAccountsView
    searchDraft: string
    setSearchDraft: SetState<string>
    searchInputRef: React.RefObject<HTMLInputElement | null>
    counterpartyPartyIdDraft: string | null
    setCounterpartyPartyIdDraft: SetState<string | null>
    statusDraft: ReceivableStatusFilter
    setStatusDraft: SetState<ReceivableStatusFilter>
    appliedChips: readonly ReceivableAppliedChip[]
    removeFilter: (key: CustomerReceivablesFilterKey) => void
    applyFilters: () => void
    clearFilters: () => void
    hasActiveFilters: boolean
    hasPendingChanges: boolean
    loading: boolean
    failed: boolean
}

export function CustomerReceivablesToolbar({
    view,
    searchDraft,
    setSearchDraft,
    searchInputRef,
    counterpartyPartyIdDraft,
    setCounterpartyPartyIdDraft,
    statusDraft,
    setStatusDraft,
    appliedChips,
    removeFilter,
    applyFilters,
    clearFilters,
    hasActiveFilters,
    hasPendingChanges,
    loading,
    failed,
}: CustomerReceivablesToolbarProps) {
    const receivableView = view === "receivable"
    const foldedKeys = receivableView
        ? FOLDED_CHIP_KEYS
        : NON_RECEIVABLE_CHIP_KEYS
    const hasFoldedChips = appliedChips.some(({ key }) =>
        foldedKeys.includes(key),
    )

    return (
        <ListWorkspaceFilterBar
            density="compact"
            idPrefix={prefix}
            formAriaLabel="客户往来查询"
            className="[&_[data-slot=list-toolbar-filters]]:min-w-0 [&_[data-slot=list-toolbar-filters]]:shrink [&_[data-slot=list-toolbar-filters]]:self-center"
            onSubmit={applyFilters}
            queryButtonId={`${prefix}-apply`}
            clearButtonId={`${prefix}-clear-all`}
            search={
                <ListSearchField
                    id={`${prefix}-search`}
                    searchInputRef={searchInputRef}
                    value={searchDraft}
                    onChange={setSearchDraft}
                    placeholder="往来主体、销售单、回款单、发票号"
                    aria-label="搜索客户往来"
                />
            }
            primaryFilters={
                <>
                    <div className="w-52 max-w-full min-w-0">
                        <ReceivableCounterpartySearchCombobox
                            id={`${prefix}-counterparty`}
                            className="w-full min-w-0"
                            filterLabel="往来主体"
                            value={counterpartyPartyIdDraft ?? undefined}
                            onValueChange={(id) =>
                                setCounterpartyPartyIdDraft(id ?? null)
                            }
                            purpose="filter"
                            aria-label="筛选往来主体"
                            placeholder="全部"
                        />
                    </div>
                    {receivableView ? (
                        <OptionCombobox
                            id={`${prefix}-status-filter`}
                            className="w-32 max-w-full min-w-0"
                            filterLabel="状态"
                            aria-label="状态"
                            value={statusDraft === "all" ? null : statusDraft}
                            onValueChange={(value) =>
                                setStatusDraft(
                                    STATUS_OPTIONS.find(
                                        (option) => option.value === value,
                                    )?.value ?? "all",
                                )
                            }
                            options={STATUS_OPTIONS}
                            placeholder="全部"
                        />
                    ) : null}
                </>
            }
            extraPrimary={
                hasActiveFilters && !hasFoldedChips ? (
                    <Button
                        id={`${prefix}-clear-inline`}
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={listWorkspaceStyles.quietButton}
                        onClick={clearFilters}
                    >
                        {listFilterText.clear}
                    </Button>
                ) : undefined
            }
            resultStatus={failed ? "查询未完成" : loading ? "查询中…" : null}
            chips={appliedChips}
            foldedChipKeys={foldedKeys}
            onClearChip={(key) =>
                removeFilter(key as CustomerReceivablesFilterKey)
            }
            onClearAll={clearFilters}
            hasPendingChanges={hasPendingChanges}
            pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
        />
    )
}
