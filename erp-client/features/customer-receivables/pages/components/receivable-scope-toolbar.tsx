"use client"

import * as React from "react"

import { FixedOptionRadioFilter } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { ResponsibleUserFilter } from "@/features/entity-selectors/components/responsible-user-filter"
import { OrganizationUnitFilter } from "@/features/organization/components/organization-unit-filter"
import type { ReceivableScopeChip } from "@/features/customer-receivables/pages/hooks/use-receivable-scope-url-state"
import type { ReceivableScopeQuery } from "@/features/customer-receivables/api/scoped"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

const prefix = "customer-receivables-scope-toolbar"
const panelId = `${prefix}-more-panel`

const OPERATOR_KIND_OPTIONS = [
    { value: "register", label: "登记" },
    { value: "settle", label: "核销" },
] as const

type Props = {
    view: ReceivableScopeQuery["view"]
    searchDraft: string
    setSearchDraft: SetState<string>
    searchInputRef: React.RefObject<HTMLInputElement | null>
    salesOwnerDraft: string
    setSalesOwnerDraft: SetState<string>
    operatorDraft: string
    setOperatorDraft: SetState<string>
    operatorKindDraft: "register" | "settle" | ""
    setOperatorKindDraft: SetState<"register" | "settle" | "">
    orgDraft: string
    setOrgDraft: SetState<string>
    descendantsDraft: boolean
    setDescendantsDraft: SetState<boolean>
    panelOpen: boolean
    setPanelOpen: SetState<boolean>
    appliedChips: readonly ReceivableScopeChip[]
    removeFilter: (key: ReceivableScopeChip["key"]) => void
    applyFilters: () => void
    resetMoreFilters: () => void
    cancelMoreFilters: () => void
    clearFilters: () => void
    hasPendingChanges: boolean
    ownerOptions: readonly { value: string; label: string }[]
    resultCount?: number
    loading: boolean
    failed: boolean
}

/** 范围查询工具栏：负责销售/经办人/组织筛选，候选可区分离线停用人员。 */
export function ReceivableScopeToolbar({
    view,
    searchDraft,
    setSearchDraft,
    searchInputRef,
    salesOwnerDraft,
    setSalesOwnerDraft,
    operatorDraft,
    setOperatorDraft,
    operatorKindDraft,
    setOperatorKindDraft,
    orgDraft,
    setOrgDraft,
    descendantsDraft,
    setDescendantsDraft,
    panelOpen,
    setPanelOpen,
    appliedChips,
    removeFilter,
    applyFilters,
    resetMoreFilters,
    cancelMoreFilters,
    clearFilters,
    hasPendingChanges,
    ownerOptions,
    resultCount,
    loading,
    failed,
}: Props) {
    const moreCount = appliedChips.filter(({ key }) =>
        ["operatorUserIds", "orgUnitIds"].includes(key),
    ).length

    return (
        <ListWorkspaceFilterBar
            density="compact"
            morePresentation="popover"
            moreSize="compact"
            className="[&_[data-slot=list-toolbar-filters]]:min-w-0 [&_[data-slot=list-toolbar-filters]]:shrink [&_[data-slot=list-toolbar-filters]]:self-center"
            idPrefix={prefix}
            formAriaLabel="客户往来范围查询"
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
                    placeholder="销售单、回款单、发票号"
                    aria-label="搜索客户往来范围"
                />
            }
            moreCount={moreCount}
            moreOpen={panelOpen}
            onToggleMore={() =>
                panelOpen ? cancelMoreFilters() : setPanelOpen(true)
            }
            morePanelId={panelId}
            morePanelAriaLabel="客户往来范围更多筛选条件"
            onResetMore={resetMoreFilters}
            primaryFilters={
                <div className="w-56 max-w-full min-w-0">
                    <ResponsibleUserFilter
                        id={`${prefix}-sales-owner`}
                        label="负责销售"
                        hideLabel
                        value={salesOwnerDraft}
                        onChange={setSalesOwnerDraft}
                        options={ownerOptions}
                    />
                </div>
            }
            morePanel={
                <div className="grid min-w-0 gap-5">
                    <ResponsibleUserFilter
                        id={`${prefix}-operator`}
                        label={
                            view === "receipt"
                                ? "登记/核销经办人"
                                : "登记经办人"
                        }
                        value={operatorDraft}
                        onChange={setOperatorDraft}
                        options={ownerOptions}
                    />
                    {view === "receipt" ? (
                        <FixedOptionRadioFilter
                            idPrefix={`${prefix}-operator-kind`}
                            label="经办人口径"
                            value={
                                operatorKindDraft === ""
                                    ? "register"
                                    : operatorKindDraft
                            }
                            onValueChange={(value) =>
                                setOperatorKindDraft(
                                    value as "register" | "settle",
                                )
                            }
                            options={OPERATOR_KIND_OPTIONS.map((option) => ({
                                ...option,
                            }))}
                        />
                    ) : null}
                    <OrganizationUnitFilter
                        id={`${prefix}-org`}
                        label="业务组织"
                        value={orgDraft}
                        onChange={setOrgDraft}
                        includeDescendants={descendantsDraft}
                        onDescendantsChange={setDescendantsDraft}
                    />
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
                removeFilter(key as ReceivableScopeChip["key"])
            }
            onClearAll={clearFilters}
            hasPendingChanges={hasPendingChanges}
            pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
        />
    )
}
