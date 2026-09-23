"use client"

import * as React from "react"

import { OptionCombobox } from "@/components/business"
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
    cancelMoreFilters: () => void
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
    cancelMoreFilters,
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
    const showPayableMore = view === "payable"
    const showTrackMore = view === "unallocated"
    const showMore = showPayableMore || showTrackMore
    const moreCount = appliedChips.filter(({ key }) =>
        showTrackMore
            ? key === "track"
            : ["supplierId", "sourceType", "paymentGate"].includes(key),
    ).length

    return (
        <ListWorkspaceFilterBar
            morePresentation="popover"
            moreSize={showPayableMore ? "wide" : "compact"}
            className="[&_[data-slot=list-toolbar-filters]]:min-w-0 [&_[data-slot=list-toolbar-filters]]:shrink [&_[data-slot=list-toolbar-filters]]:self-center"
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
                showMore
                    ? () =>
                          panelOpen ? cancelMoreFilters() : setPanelOpen(true)
                    : undefined
            }
            morePanelId={panelId}
            morePanelAriaLabel="供应商往来更多筛选条件"
            onResetMore={showMore ? resetMoreFilters : undefined}
            primaryFilters={
                showPayableMore ? (
                    <>
                        <OptionCombobox
                            id={`${prefix}-filter-status`}
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
                            options={STATUS_OPTIONS.filter(
                                (option) => option.value !== "all",
                            )}
                            placeholder="全部"
                        />
                        <OptionCombobox
                            id={`${prefix}-filter-due`}
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
                            options={DUE_OPTIONS.filter(
                                (option) => option.value !== "all",
                            )}
                            placeholder="全部"
                        />
                    </>
                ) : null
            }
            morePanel={
                showMore ? (
                    <div className="space-y-5">
                        <fieldset className="min-w-0 space-y-3">
                            <legend className="mb-1 text-sm font-medium">
                                进度
                            </legend>
                            {showTrackMore ? (
                                <ListWorkspaceFilterField
                                    htmlFor={`${prefix}-filter-track`}
                                    label="轨道"
                                >
                                    <OptionCombobox
                                        id={`${prefix}-filter-track`}
                                        className="w-full min-w-0"
                                        aria-label="轨道"
                                        value={
                                            trackDraft === "all"
                                                ? null
                                                : trackDraft
                                        }
                                        onValueChange={(value) =>
                                            setTrackDraft(
                                                TRACK_OPTIONS.find(
                                                    (option) =>
                                                        option.value === value,
                                                )?.value ?? "all",
                                            )
                                        }
                                        options={TRACK_OPTIONS.filter(
                                            (option) => option.value !== "all",
                                        )}
                                        placeholder="全部"
                                    />
                                </ListWorkspaceFilterField>
                            ) : (
                                <ListWorkspaceFilterField
                                    htmlFor={`${prefix}-filter-payment-gate`}
                                    label="先款条件"
                                >
                                    <OptionCombobox
                                        id={`${prefix}-filter-payment-gate`}
                                        className="w-full min-w-0"
                                        aria-label="先款条件"
                                        value={
                                            paymentGateDraft === "all"
                                                ? null
                                                : paymentGateDraft
                                        }
                                        onValueChange={(value) =>
                                            setPaymentGateDraft(
                                                PAYMENT_GATE_OPTIONS.find(
                                                    (option) =>
                                                        option.value === value,
                                                )?.value ?? "all",
                                            )
                                        }
                                        options={PAYMENT_GATE_OPTIONS.filter(
                                            (option) => option.value !== "all",
                                        )}
                                        placeholder="全部"
                                    />
                                </ListWorkspaceFilterField>
                            )}
                        </fieldset>
                        {showPayableMore ? (
                            <fieldset className="min-w-0 space-y-3 border-t pt-4">
                                <legend className="pr-2 text-sm font-medium">
                                    对象
                                </legend>
                                <div className="grid min-w-0 gap-3 sm:grid-cols-2">
                                    <ListWorkspaceFilterField
                                        htmlFor={`${prefix}-supplier-filter`}
                                        label="供应商"
                                    >
                                        <SupplierSearchCombobox
                                            id={`${prefix}-supplier-filter`}
                                            className="w-full min-w-0"
                                            value={supplierDraft ?? undefined}
                                            onValueChange={(id) =>
                                                setSupplierDraft(id ?? null)
                                            }
                                            purpose="filter"
                                            aria-label="供应商"
                                            placeholder="全部供应商"
                                        />
                                    </ListWorkspaceFilterField>
                                    <ListWorkspaceFilterField
                                        htmlFor={`${prefix}-filter-source-type`}
                                        label="来源类型"
                                    >
                                        <OptionCombobox
                                            id={`${prefix}-filter-source-type`}
                                            className="w-full min-w-0"
                                            aria-label="来源类型"
                                            value={
                                                sourceTypeDraft === "all"
                                                    ? null
                                                    : sourceTypeDraft
                                            }
                                            onValueChange={(value) =>
                                                setSourceTypeDraft(
                                                    SOURCE_TYPE_OPTIONS.find(
                                                        (option) =>
                                                            option.value ===
                                                            value,
                                                    )?.value ?? "all",
                                                )
                                            }
                                            options={SOURCE_TYPE_OPTIONS.filter(
                                                (option) =>
                                                    option.value !== "all",
                                            )}
                                            placeholder="全部"
                                        />
                                    </ListWorkspaceFilterField>
                                </div>
                            </fieldset>
                        ) : null}
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
