"use client"

import * as React from "react"

import { OptionCombobox } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { ReceivableCounterpartySearchCombobox } from "@/features/customer-receivables/components/receivable-counterparty-search-combobox"
import {
    DUE_LABEL,
    RECEIVABLE_STATUS_LABEL,
    type CustomerAccountsView,
    type CustomerReceivablesFilterKey,
    type DueFilter,
    type ReceivableStatusFilter,
} from "@/features/customer-receivables/types"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

export type ReceivableAppliedChip = Readonly<{
    key: CustomerReceivablesFilterKey
    label: string
}>

const prefix = "customer-receivables-toolbar"
const panelId = `${prefix}-more-panel`

const DUE_OPTIONS = (["not_due", "due_today", "overdue"] as const).map(
    (value) => ({
        value,
        label: DUE_LABEL[value],
    }),
)

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
    dueDraft: DueFilter
    setDueDraft: SetState<DueFilter>
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
    resultCount?: number
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
    dueDraft,
    setDueDraft,
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
    resultCount,
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
            moreSize="compact"
            className="[&_[data-slot=list-toolbar-filters]]:min-w-0 [&_[data-slot=list-toolbar-filters]]:shrink [&_[data-slot=list-toolbar-filters]]:self-center"
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
                    <>
                        <OptionCombobox
                            id={`${prefix}-status-filter`}
                            className="w-48 max-w-full min-w-0"
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
                        <OptionCombobox
                            id={`${prefix}-due-filter`}
                            className="w-48 max-w-full min-w-0"
                            filterLabel="到期"
                            aria-label="到期"
                            value={dueDraft === "all" ? null : dueDraft}
                            onValueChange={(value) =>
                                setDueDraft(
                                    DUE_OPTIONS.find(
                                        (option) => option.value === value,
                                    )?.value ?? "all",
                                )
                            }
                            options={DUE_OPTIONS}
                            placeholder="全部"
                        />
                    </>
                ) : null
            }
            morePanel={
                receivableView ? (
                    <ListWorkspaceFilterField
                        htmlFor={`${prefix}-counterparty`}
                        label="往来主体"
                    >
                        <ReceivableCounterpartySearchCombobox
                            id={`${prefix}-counterparty`}
                            className="w-full min-w-0"
                            value={counterpartyPartyIdDraft ?? undefined}
                            onValueChange={(id) =>
                                setCounterpartyPartyIdDraft(id ?? null)
                            }
                            purpose="filter"
                            aria-label="筛选往来主体"
                            placeholder="全部主体"
                        />
                    </ListWorkspaceFilterField>
                ) : undefined
            }
            resultStatus={listWorkspaceFilterStatusText({
                loading,
                failed,
                resultCount,
                noun: "条往来",
                loadingLabel: "正在加载往来…",
            })}
            chips={appliedChips}
            onClearChip={(key) =>
                removeFilter(key as CustomerReceivablesFilterKey)
            }
            onClearAll={clearFilters}
            hasPendingChanges={hasPendingChanges}
            pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
        />
    )
}
