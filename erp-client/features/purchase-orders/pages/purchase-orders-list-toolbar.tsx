"use client"

import * as React from "react"

import { OptionCombobox } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceInlineFilter,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import type {
    PurchaseOrderAppliedChip,
    PurchaseOrderFilterKey,
} from "@/features/purchase-orders/hooks/use-purchase-orders-list-filters"
import type { PurchaseOrderStatusFilter } from "@/features/purchase-orders/types"
import { PO_STATUS_FILTER_LABEL } from "@/features/purchase-orders/types"

/** 状态枚举 ≥5：面板内用 Combobox，禁止长 Toggle 横排。 */
const PO_STATUS_FILTER_OPTIONS = (
    Object.entries(PO_STATUS_FILTER_LABEL) as Array<
        [PurchaseOrderStatusFilter, string]
    >
)
    .filter(([value]) => value !== "all")
    .map(([value, label]) => ({ value, label }))

const prefix = "procurement-orders-list"

export type PurchaseOrdersListToolbarProps = {
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: React.Dispatch<React.SetStateAction<string>>
    statusDraft: PurchaseOrderStatusFilter
    setStatusDraft: React.Dispatch<
        React.SetStateAction<PurchaseOrderStatusFilter>
    >
    appliedChips: readonly PurchaseOrderAppliedChip[]
    removeFilter: (key: PurchaseOrderFilterKey) => void
    applyFilters: () => void
    clearAllFilters: () => void
    hasPendingChanges: boolean
    resultCount?: number
    loading: boolean
    failed: boolean
}

export function PurchaseOrdersListToolbar({
    searchInputRef,
    searchDraft,
    setSearchDraft,
    statusDraft,
    setStatusDraft,
    appliedChips,
    removeFilter,
    applyFilters,
    clearAllFilters,
    hasPendingChanges,
    resultCount,
    loading,
    failed,
}: PurchaseOrdersListToolbarProps) {
    return (
        <ListWorkspaceFilterBar
            idPrefix={`${prefix}-filter`}
            formAriaLabel="采购单查询"
            onSubmit={applyFilters}
            search={
                <ListSearchField
                    id={`${prefix}-search`}
                    searchInputRef={searchInputRef}
                    data-slot="po-list-search"
                    value={searchDraft}
                    onChange={setSearchDraft}
                    placeholder="采购单号、供应商、来源销售单"
                    aria-label="搜索采购单"
                />
            }
            queryButtonId={`${prefix}-apply-filters`}
            clearButtonId={`${prefix}-clear-all`}
            commonFilters={
                <ListWorkspaceInlineFilter
                    htmlFor={`${prefix}-status-filter`}
                    label="主状态"
                >
                    <OptionCombobox
                        id={`${prefix}-status-filter`}
                        className="w-full sm:w-60"
                        value={statusDraft === "all" ? null : statusDraft}
                        onValueChange={(value) =>
                            setStatusDraft(
                                (value as PurchaseOrderStatusFilter | null) ??
                                    "all",
                            )
                        }
                        options={PO_STATUS_FILTER_OPTIONS}
                        aria-label="主状态"
                        placeholder="状态：全部"
                    />
                </ListWorkspaceInlineFilter>
            }
            resultStatus={listWorkspaceFilterStatusText({
                loading,
                failed,
                resultCount,
                noun: "张采购单",
                loadingLabel: "正在加载采购单…",
            })}
            chips={appliedChips}
            onClearChip={(key) => removeFilter(key as PurchaseOrderFilterKey)}
            onClearAll={clearAllFilters}
            hasPendingChanges={hasPendingChanges}
            pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
            idleHint="导出与当前查询结果一致"
        />
    )
}
