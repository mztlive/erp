"use client"
import {
    ResponsibleUserFilter,
    type ResponsibleUserOption,
} from "@/features/entity-selectors/components/responsible-user-filter"

import * as React from "react"

import {
    ListSearchField,
    ListWorkspaceFilterBar,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import type {
    PurchaseOrderAppliedChip,
    PurchaseOrderFilterKey,
} from "@/features/purchase-orders/hooks/use-purchase-orders-list-filters"
const prefix = "procurement-orders-list"

export type PurchaseOrdersListToolbarProps = {
    ownerDraft: string
    setOwnerDraft: (value: string) => void
    ownerOptions: readonly ResponsibleUserOption[]
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: React.Dispatch<React.SetStateAction<string>>
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
    ownerDraft,
    setOwnerDraft,
    ownerOptions,
    searchInputRef,
    searchDraft,
    setSearchDraft,
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
            commonFilters={
                <ResponsibleUserFilter
                    id="procurement-orders-list-owner"
                    label="采购负责人"
                    value={ownerDraft}
                    onChange={setOwnerDraft}
                    options={ownerOptions}
                />
            }
            queryButtonId={`${prefix}-apply-filters`}
            clearButtonId={`${prefix}-clear-all`}
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
