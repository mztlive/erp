"use client"

import * as React from "react"

import { OptionCombobox } from "@/components/business"
import {
    ListFilterSection,
    ListFilterSections,
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
} from "@/components/business/list-workspace"
import { ReceivableCounterpartySearchCombobox } from "@/features/customer-receivables/components/receivable-counterparty-search-combobox"
import {
    RECEIVABLE_STATUS_LABEL,
    type CustomerAccountsView,
    type CustomerReceivablesFilterKey,
    type ReceivableStatusFilter,
} from "@/features/customer-receivables/types"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

export type ReceivableAppliedChip = Readonly<{
    key: CustomerReceivablesFilterKey
    label: string
}>

const prefix = "customer-receivables-toolbar"
const panelId = `${prefix}-more-panel`
const FOLDED_CHIP_KEYS = [
    "counterpartyId",
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
    panelOpen: boolean
    setPanelOpen: SetState<boolean>
    appliedChips: readonly ReceivableAppliedChip[]
    removeFilter: (key: CustomerReceivablesFilterKey) => void
    applyFilters: () => void
    resetMoreFilters: () => void
    cancelMoreFilters: () => void
    clearFilters: () => void
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
    panelOpen,
    setPanelOpen,
    appliedChips,
    removeFilter,
    applyFilters,
    resetMoreFilters,
    cancelMoreFilters,
    clearFilters,
    hasPendingChanges,
    loading,
    failed,
}: CustomerReceivablesToolbarProps) {
    const receivableView = view === "receivable"
    const moreCount = appliedChips.filter(
        ({ key }) => key === "counterpartyId",
    ).length

    return (
        <ListWorkspaceFilterBar
            density="compact"
            morePresentation="popover"
            idPrefix={prefix}
            formAriaLabel="客户往来查询"
            onSubmit={applyFilters}
            queryButtonId={`${prefix}-apply`}
            moreButtonId={`${prefix}-more-filters`}
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
            moreCount={moreCount}
            moreOpen={panelOpen}
            onToggleMore={
                receivableView
                    ? () =>
                          panelOpen ? cancelMoreFilters() : setPanelOpen(true)
                    : undefined
            }
            morePanelId={panelId}
            morePanelAriaLabel="客户往来更多筛选条件"
            onResetMore={receivableView ? resetMoreFilters : undefined}
            primaryFilters={
                receivableView ? (
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
                ) : null
            }
            morePanel={
                receivableView ? (
                    <ListFilterSections>
                        <ListFilterSection legend="归属">
                            <ListWorkspaceFilterField
                                htmlFor={`${prefix}-counterparty`}
                                label="往来主体"
                            >
                                <ReceivableCounterpartySearchCombobox
                                    id={`${prefix}-counterparty`}
                                    className="w-full min-w-0"
                                    value={
                                        counterpartyPartyIdDraft ?? undefined
                                    }
                                    onValueChange={(id) =>
                                        setCounterpartyPartyIdDraft(id ?? null)
                                    }
                                    purpose="filter"
                                    aria-label="筛选往来主体"
                                    placeholder="全部"
                                />
                            </ListWorkspaceFilterField>
                        </ListFilterSection>
                        <ListFilterSection legend="到期">
                            <ListWorkspaceFilterField
                                htmlFor={`${prefix}-due-filter`}
                                label="到期状态"
                            >
                                <OptionCombobox
                                    id={`${prefix}-due-filter`}
                                    className="w-full min-w-0"
                                    aria-label="到期状态（暂不支持筛选）"
                                    placeholder="暂不支持筛选"
                                    value={null}
                                    onValueChange={() => {}}
                                    options={[]}
                                    disabled
                                />
                            </ListWorkspaceFilterField>
                        </ListFilterSection>
                    </ListFilterSections>
                ) : undefined
            }
            resultStatus={failed ? "查询未完成" : loading ? "查询中…" : null}
            chips={appliedChips}
            foldedChipKeys={
                receivableView ? FOLDED_CHIP_KEYS : NON_RECEIVABLE_CHIP_KEYS
            }
            onClearChip={(key) =>
                removeFilter(key as CustomerReceivablesFilterKey)
            }
            onClearAll={clearFilters}
            hasPendingChanges={hasPendingChanges}
            pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
        />
    )
}
