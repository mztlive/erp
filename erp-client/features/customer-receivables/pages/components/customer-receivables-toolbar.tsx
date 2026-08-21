"use client"

import * as React from "react"
import { ChevronDownIcon, FilterIcon, SearchIcon } from "lucide-react"

import {
    FilterChip,
    FixedOptionRadioFilter,
    ListToolbar,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { ReceivableCounterpartySearchCombobox } from "@/features/customer-receivables/components/receivable-counterparty-search-combobox"
import {
    DUE_LABEL,
    type CustomerAccountsView,
    type CustomerReceivablesFilterKey,
    type DueFilter,
    type ReceivableReviewStatusFilter,
    type ReceivableStatusFilter,
} from "@/features/customer-receivables/types"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

export type ReceivableAppliedChip = Readonly<{
    key: CustomerReceivablesFilterKey
    label: string
}>

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

const REVIEW_STATUS_RADIO_OPTIONS: ReadonlyArray<{
    value: ReceivableReviewStatusFilter
    label: string
}> = (
    [
        { value: "all", label: "全部复核状态" },
        { value: "pending_opening", label: "期初待复核" },
        { value: "reviewed", label: "已复核" },
        { value: "pending_sync_diff", label: "同步差额待复核" },
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
    reviewStatusDraft: ReceivableReviewStatusFilter
    setReviewStatusDraft: SetState<ReceivableReviewStatusFilter>
    panelOpen: boolean
    setPanelOpen: SetState<boolean>
    hasStructuredFilters: boolean
    hasActiveFilters: boolean
    appliedChips: readonly ReceivableAppliedChip[]
    removeFilter: (key: CustomerReceivablesFilterKey) => void
    applyFilters: () => void
    resetMoreFilters: () => void
    clearFilters: () => void
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
    reviewStatusDraft,
    setReviewStatusDraft,
    panelOpen,
    setPanelOpen,
    hasStructuredFilters,
    hasActiveFilters,
    appliedChips,
    removeFilter,
    applyFilters,
    resetMoreFilters,
    clearFilters,
}: CustomerReceivablesToolbarProps) {
    const panelId = React.useId()
    const receivableView = view === "receivable"
    const hasChips = hasActiveFilters && appliedChips.length > 0

    return (
        <form
            onSubmit={(event) => {
                event.preventDefault()
                applyFilters()
            }}
        >
            <ListToolbar
                search={
                    <InputGroup>
                        <InputGroupAddon>
                            <SearchIcon aria-hidden="true" />
                        </InputGroupAddon>
                        <InputGroupInput
                            ref={searchInputRef}
                            value={searchDraft}
                            onChange={(event) =>
                                setSearchDraft(event.target.value)
                            }
                            placeholder="往来主体、销售单、回款单、发票号"
                            aria-label="搜索客户往来"
                        />
                    </InputGroup>
                }
                filters={
                    <Button
                        type="button"
                        variant="outline"
                        aria-expanded={panelOpen}
                        aria-controls={panelId}
                        onClick={() => setPanelOpen((open) => !open)}
                    >
                        <FilterIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        更多筛选
                        {hasStructuredFilters ? (
                            <Badge variant="info">已启用</Badge>
                        ) : null}
                        <ChevronDownIcon
                            data-icon="inline-end"
                            aria-hidden="true"
                            className={
                                panelOpen
                                    ? "rotate-180 transition-transform"
                                    : "transition-transform"
                            }
                        />
                    </Button>
                }
                secondary={
                    hasChips || panelOpen ? (
                        <div className="w-full space-y-3">
                            {hasChips ? (
                                <div className="flex flex-wrap items-center gap-2 border-t pt-3">
                                    <span className="text-xs text-muted-foreground">
                                        已筛选
                                    </span>
                                    {appliedChips.map((chip) => (
                                        <FilterChip
                                            key={chip.key}
                                            label={chip.label}
                                            clearLabel={`移除${chip.label}`}
                                            onClear={() =>
                                                removeFilter(chip.key)
                                            }
                                        />
                                    ))}
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        onClick={clearFilters}
                                    >
                                        清空全部
                                    </Button>
                                </div>
                            ) : null}
                            {panelOpen ? (
                                <div
                                    id={panelId}
                                    className="flex w-full flex-col gap-3 border-t pt-3"
                                    aria-label="客户往来更多筛选条件"
                                >
                                    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                往来主体
                                            </span>
                                            <ReceivableCounterpartySearchCombobox
                                                className="w-full"
                                                value={
                                                    counterpartyPartyIdDraft ??
                                                    undefined
                                                }
                                                onValueChange={(id) =>
                                                    setCounterpartyPartyIdDraft(
                                                        id ?? null,
                                                    )
                                                }
                                                purpose="filter"
                                                aria-label="筛选往来主体"
                                                placeholder="全部主体"
                                            />
                                        </div>
                                    </div>
                                    {receivableView ? (
                                        <>
                                            <FixedOptionRadioFilter
                                                label="到期"
                                                value={dueDraft}
                                                onValueChange={setDueDraft}
                                                options={DUE_RADIO_OPTIONS}
                                            />
                                            <FixedOptionRadioFilter
                                                label="状态"
                                                value={statusDraft}
                                                onValueChange={setStatusDraft}
                                                options={STATUS_RADIO_OPTIONS}
                                            />
                                            <FixedOptionRadioFilter
                                                label="复核状态"
                                                value={reviewStatusDraft}
                                                onValueChange={
                                                    setReviewStatusDraft
                                                }
                                                options={
                                                    REVIEW_STATUS_RADIO_OPTIONS
                                                }
                                            />
                                        </>
                                    ) : null}
                                    <div className="flex flex-col gap-3 border-t pt-3 sm:flex-row sm:items-center sm:justify-between">
                                        <p className="text-xs text-muted-foreground">
                                            将同时应用上方关键词和以下筛选条件；结果也用于导出。
                                        </p>
                                        <div className="flex flex-wrap items-center gap-2 sm:justify-end">
                                            <Button
                                                type="button"
                                                variant="ghost"
                                                onClick={resetMoreFilters}
                                            >
                                                重置更多条件
                                            </Button>
                                            <Button type="submit">
                                                <SearchIcon
                                                    data-icon="inline-start"
                                                    aria-hidden="true"
                                                />
                                                应用全部筛选
                                            </Button>
                                        </div>
                                    </div>
                                </div>
                            ) : null}
                        </div>
                    ) : undefined
                }
            />
        </form>
    )
}
