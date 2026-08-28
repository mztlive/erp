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
import { SupplierSearchCombobox } from "@/features/entity-selectors"
import type {
    AllocationTrack,
    PayableSourceType,
    SupplierAccountsView,
} from "@/features/supplier-payables/types"

/** 可被单独移除的已生效条件。 */
export type SupplierFilterKey =
    | "q"
    | "supplierId"
    | "sourceType"
    | "status"
    | "due"
    | "paymentGate"
    | "track"
    | "purchaseOrderId"

export type SupplierAppliedChip = Readonly<{
    key: SupplierFilterKey
    label: string
}>

const SOURCE_TYPE_OPTIONS: ReadonlyArray<{
    value: PayableSourceType | "all"
    label: string
}> = [
    { value: "all", label: "全部" },
    { value: "PURCHASE_ORDER", label: "采购单" },
    { value: "SUPPLIER_SETTLEMENT", label: "供应商结算单" },
]

const STATUS_OPTIONS: ReadonlyArray<{
    value: "OPEN" | "PARTIAL" | "SETTLED" | "all"
    label: string
}> = [
    { value: "all", label: "全部" },
    { value: "OPEN", label: "未结" },
    { value: "PARTIAL", label: "部分结清" },
    { value: "SETTLED", label: "已结清" },
]

const DUE_OPTIONS: ReadonlyArray<{
    value: "not_due" | "due_today" | "overdue" | "all"
    label: string
}> = [
    { value: "all", label: "全部" },
    { value: "not_due", label: "未到期" },
    { value: "due_today", label: "今日到期" },
    { value: "overdue", label: "已到期" },
]

const PAYMENT_GATE_OPTIONS: ReadonlyArray<{
    value: "satisfied" | "unsatisfied" | "all"
    label: string
}> = [
    { value: "all", label: "全部" },
    { value: "satisfied", label: "已满足" },
    { value: "unsatisfied", label: "未满足" },
]

const TRACK_OPTIONS: ReadonlyArray<{
    value: AllocationTrack | "all"
    label: string
}> = [
    { value: "all", label: "全部" },
    { value: "payment", label: "付款" },
    { value: "purchase_invoice", label: "进项票" },
]

export interface SupplierAccountsToolbarProps {
    view: SupplierAccountsView
    searchInput: string
    onSearchInputChange: (value: string) => void
    searchInputRef: React.Ref<HTMLInputElement>
    hasActiveFilters: boolean
    hasStructuredFilters: boolean
    panelOpen: boolean
    setPanelOpen: React.Dispatch<React.SetStateAction<boolean>>
    appliedChips: readonly SupplierAppliedChip[]
    applyFilters: () => void
    resetMoreFilters: () => void
    clearAllFilters: () => void
    removeFilter: (key: SupplierFilterKey) => void
    supplierDraft: string | null
    setSupplierDraft: (value: string | null) => void
    sourceTypeDraft: PayableSourceType | "all"
    setSourceTypeDraft: (value: PayableSourceType | "all") => void
    statusDraft: "OPEN" | "PARTIAL" | "SETTLED" | "all"
    setStatusDraft: (value: "OPEN" | "PARTIAL" | "SETTLED" | "all") => void
    dueDraft: "not_due" | "due_today" | "overdue" | "all"
    setDueDraft: (value: "not_due" | "due_today" | "overdue" | "all") => void
    paymentGateDraft: "satisfied" | "unsatisfied" | "all"
    setPaymentGateDraft: (value: "satisfied" | "unsatisfied" | "all") => void
    trackDraft: AllocationTrack | "all"
    setTrackDraft: (value: AllocationTrack | "all") => void
}

export function SupplierAccountsToolbar({
    view,
    searchInput,
    onSearchInputChange,
    searchInputRef,
    hasActiveFilters,
    hasStructuredFilters,
    panelOpen,
    setPanelOpen,
    appliedChips,
    applyFilters,
    resetMoreFilters,
    clearAllFilters,
    removeFilter,
    supplierDraft,
    setSupplierDraft,
    sourceTypeDraft,
    setSourceTypeDraft,
    statusDraft,
    setStatusDraft,
    dueDraft,
    setDueDraft,
    paymentGateDraft,
    setPaymentGateDraft,
    trackDraft,
    setTrackDraft,
}: SupplierAccountsToolbarProps) {
    const panelId = React.useId()
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
                            placeholder="供应商、采购单、结算单、付款单、发票号"
                            value={searchInput}
                            onChange={(e) =>
                                onSearchInputChange(e.target.value)
                            }
                            aria-label="搜索供应商往来"
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
                                        onClick={clearAllFilters}
                                    >
                                        清空全部
                                    </Button>
                                </div>
                            ) : null}
                            {panelOpen ? (
                                <div
                                    id={panelId}
                                    className="flex w-full flex-col gap-3 border-t pt-3"
                                    aria-label="供应商往来更多筛选条件"
                                >
                                    <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                        <span className="text-muted-foreground">
                                            供应商
                                        </span>
                                        <SupplierSearchCombobox
                                            className="w-full"
                                            value={supplierDraft ?? undefined}
                                            onValueChange={(id) =>
                                                setSupplierDraft(id ?? null)
                                            }
                                            purpose="filter"
                                            aria-label="供应商"
                                            placeholder="全部供应商"
                                        />
                                    </div>
                                    {view === "payable" ? (
                                        <>
                                            <FixedOptionRadioFilter
                                                label="来源类型"
                                                value={sourceTypeDraft}
                                                onValueChange={
                                                    setSourceTypeDraft
                                                }
                                                options={SOURCE_TYPE_OPTIONS}
                                            />
                                            <FixedOptionRadioFilter
                                                label="状态"
                                                value={statusDraft}
                                                onValueChange={setStatusDraft}
                                                options={STATUS_OPTIONS}
                                            />
                                            <FixedOptionRadioFilter
                                                label="到期"
                                                value={dueDraft}
                                                onValueChange={setDueDraft}
                                                options={DUE_OPTIONS}
                                            />
                                            <FixedOptionRadioFilter
                                                label="先款条件"
                                                value={paymentGateDraft}
                                                onValueChange={
                                                    setPaymentGateDraft
                                                }
                                                options={PAYMENT_GATE_OPTIONS}
                                            />
                                        </>
                                    ) : null}
                                    {view === "unallocated" ? (
                                        <FixedOptionRadioFilter
                                            label="轨道"
                                            value={trackDraft}
                                            onValueChange={setTrackDraft}
                                            options={TRACK_OPTIONS}
                                        />
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
