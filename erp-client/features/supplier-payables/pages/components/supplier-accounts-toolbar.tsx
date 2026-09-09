"use client"

import * as React from "react"

import { FixedOptionRadioFilter } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
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

const prefix = "supplier-payables-toolbar"
const panelId = `${prefix}-more-panel`

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
    hasPendingChanges: boolean
    resultCount?: number
    loading: boolean
    failed: boolean
}

export function SupplierAccountsToolbar({
    view,
    searchInput,
    onSearchInputChange,
    searchInputRef,
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
    hasPendingChanges,
    resultCount,
    loading,
    failed,
}: SupplierAccountsToolbarProps) {
    const moreCount = appliedChips.filter(({ key }) =>
        ["supplierId", "sourceType", "paymentGate"].includes(key),
    ).length

    return (
        <ListWorkspaceFilterBar
            idPrefix={prefix}
            formAriaLabel="供应商往来查询"
            onSubmit={applyFilters}
            queryButtonId={`${prefix}-apply`}
            moreButtonId={`${prefix}-filter-toggle`}
            clearButtonId={`${prefix}-clear-all`}
            search={
                <ListSearchField
                    id={`${prefix}-search`}
                    searchInputRef={searchInputRef}
                    value={searchInput}
                    onChange={onSearchInputChange}
                    placeholder="供应商、采购单、结算单、付款单、发票号"
                    aria-label="搜索供应商往来"
                />
            }
            moreCount={moreCount}
            moreOpen={panelOpen}
            onToggleMore={
                view === "payable"
                    ? () => setPanelOpen((open) => !open)
                    : undefined
            }
            morePanelId={panelId}
            morePanelAriaLabel="供应商往来更多筛选条件"
            onResetMore={resetMoreFilters}
            commonFilters={
                view === "payable" ? (
                    <>
                        <FixedOptionRadioFilter
                            idPrefix={`${prefix}-filter-status`}
                            label="状态"
                            variant="quiet"
                            value={statusDraft}
                            onValueChange={setStatusDraft}
                            options={STATUS_OPTIONS}
                        />
                        <FixedOptionRadioFilter
                            idPrefix={`${prefix}-filter-due`}
                            label="到期"
                            variant="quiet"
                            value={dueDraft}
                            onValueChange={setDueDraft}
                            options={DUE_OPTIONS}
                        />
                    </>
                ) : view === "unallocated" ? (
                    <FixedOptionRadioFilter
                        idPrefix={`${prefix}-filter-track`}
                        label="轨道"
                        variant="quiet"
                        value={trackDraft}
                        onValueChange={setTrackDraft}
                        options={TRACK_OPTIONS}
                    />
                ) : null
            }
            morePanel={
                view === "payable" ? (
                    <div className="grid min-w-0 gap-5">
                        <ListWorkspaceFilterField
                            htmlFor={`${prefix}-supplier-filter`}
                            label="供应商"
                        >
                            <SupplierSearchCombobox
                                id={`${prefix}-supplier-filter`}
                                className="w-full sm:w-60"
                                value={supplierDraft ?? undefined}
                                onValueChange={(id) =>
                                    setSupplierDraft(id ?? null)
                                }
                                purpose="filter"
                                aria-label="供应商"
                                placeholder="全部供应商"
                            />
                        </ListWorkspaceFilterField>
                        <FixedOptionRadioFilter
                            idPrefix={`${prefix}-filter-source-type`}
                            label="来源类型"
                            value={sourceTypeDraft}
                            onValueChange={setSourceTypeDraft}
                            options={SOURCE_TYPE_OPTIONS}
                        />
                        <FixedOptionRadioFilter
                            idPrefix={`${prefix}-filter-payment-gate`}
                            label="先款条件"
                            value={paymentGateDraft}
                            onValueChange={setPaymentGateDraft}
                            options={PAYMENT_GATE_OPTIONS}
                        />
                    </div>
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
            onClearChip={(key) => removeFilter(key as SupplierFilterKey)}
            onClearAll={clearAllFilters}
            hasPendingChanges={hasPendingChanges}
            pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
            idleHint="导出与当前查询结果一致"
        />
    )
}
