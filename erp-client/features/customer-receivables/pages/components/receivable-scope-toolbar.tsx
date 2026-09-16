"use client"

import * as React from "react"

import { FixedOptionRadioFilter } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { ResponsibleUserFilter } from "@/features/entity-selectors/components/responsible-user-filter"
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
    clearFilters,
    hasPendingChanges,
    ownerOptions,
    resultCount,
    loading,
    failed,
}: Props) {
    const moreCount = appliedChips.filter(({ key }) =>
        [
            "salesOwnerUserIds",
            "operatorUserIds",
            "operatorKind",
            "orgUnitIds",
        ].includes(key),
    ).length

    return (
        <ListWorkspaceFilterBar
            density="compact"
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
            onToggleMore={() => setPanelOpen((open) => !open)}
            morePanelId={panelId}
            morePanelAriaLabel="客户往来范围更多筛选条件"
            onResetMore={resetMoreFilters}
            morePanel={
                <div className="grid min-w-0 gap-5">
                    <ResponsibleUserFilter
                        id={`${prefix}-sales-owner`}
                        label="负责销售"
                        value={salesOwnerDraft}
                        onChange={setSalesOwnerDraft}
                        options={ownerOptions}
                    />
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
                    <ListWorkspaceFilterField
                        htmlFor={`${prefix}-org`}
                        label="业务组织（逗号分隔组织 ID）"
                    >
                        <input
                            id={`${prefix}-org`}
                            className="h-9 w-full min-w-0 rounded-md border bg-background px-3 text-sm"
                            value={orgDraft}
                            onChange={(event) =>
                                setOrgDraft(event.target.value)
                            }
                            placeholder="全部组织"
                            aria-label="筛选业务组织"
                        />
                    </ListWorkspaceFilterField>
                    <label
                        htmlFor={`${prefix}-include-descendants`}
                        className="flex min-w-0 items-center gap-2 text-sm"
                    >
                        <input
                            id={`${prefix}-include-descendants`}
                            type="checkbox"
                            checked={descendantsDraft}
                            onChange={(event) =>
                                setDescendantsDraft(event.target.checked)
                            }
                        />
                        包含下级组织
                    </label>
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
