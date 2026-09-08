"use client"

import * as React from "react"

import { FixedOptionRadioFilter } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { ReceivableCounterpartySearchCombobox } from "@/features/customer-receivables/components/receivable-counterparty-search-combobox"
import {
    DUE_LABEL,
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

const DUE_RADIO_OPTIONS: ReadonlyArray<{
    value: DueFilter
    label: string
}> = (["all", "not_due", "due_today", "overdue"] as const).map((value) => ({
    value,
    label: DUE_LABEL[value],
}))

const STATUS_RADIO_OPTIONS: ReadonlyArray<{
    value: ReceivableStatusFilter
    label: string
}> = (
    [
        { value: "all", label: "全部状态" },
        { value: "open", label: "未结" },
        { value: "partial", label: "部分结清" },
        { value: "settled", label: "已结清" },
    ] as const
).map((option) => ({ ...option }))

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
    clearFilters,
    hasPendingChanges,
    resultCount,
    loading,
    failed,
}: CustomerReceivablesToolbarProps) {
    const receivableView = view === "receivable"
    const moreCount = appliedChips.filter(({ key }) =>
        ["counterpartyId", "status"].includes(key),
    ).length

    return (
        <ListWorkspaceFilterBar
            density="compact"
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
            onToggleMore={() => setPanelOpen((open) => !open)}
            morePanelId={panelId}
            morePanelAriaLabel="客户往来更多筛选条件"
            onResetMore={resetMoreFilters}
            commonFilters={
                receivableView ? (
                    <FixedOptionRadioFilter
                        idPrefix={`${prefix}-due-filter`}
                        label="到期"
                        variant="quiet"
                        value={dueDraft}
                        onValueChange={setDueDraft}
                        options={DUE_RADIO_OPTIONS}
                    />
                ) : null
            }
            morePanel={
                <div className="grid min-w-0 gap-5">
                    <ListWorkspaceFilterField
                        htmlFor={`${prefix}-counterparty`}
                        label="往来主体"
                    >
                        <ReceivableCounterpartySearchCombobox
                            id={`${prefix}-counterparty`}
                            className="w-full sm:w-60"
                            value={counterpartyPartyIdDraft ?? undefined}
                            onValueChange={(id) =>
                                setCounterpartyPartyIdDraft(id ?? null)
                            }
                            purpose="filter"
                            aria-label="筛选往来主体"
                            placeholder="全部主体"
                        />
                    </ListWorkspaceFilterField>
                    {receivableView ? (
                        <>
                            <FixedOptionRadioFilter
                                idPrefix={`${prefix}-status-filter`}
                                label="状态"
                                value={statusDraft}
                                onValueChange={setStatusDraft}
                                options={STATUS_RADIO_OPTIONS}
                            />
                        </>
                    ) : null}
                </div>
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
