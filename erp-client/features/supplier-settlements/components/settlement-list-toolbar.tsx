"use client"

import * as React from "react"

import {
    FixedOptionRadioFilter,
    MultiOptionCombobox,
} from "@/components/business"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { ResponsibleUserFilter } from "@/features/entity-selectors/components/responsible-user-filter"
import type { SettlementFilterOption } from "@/features/supplier-settlements/types"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { DatePicker } from "@/components/ui/date-picker"
import { SupplierSearchCombobox } from "@/features/entity-selectors"
import type { SettlementsUrlState } from "@/features/supplier-settlements/lib/url-state"
import {
    buildSettlementFilterChips,
    DIFF_TYPE_RADIO_OPTIONS,
    SETTLEMENT_STATUS_VALUES,
    type SettlementFilterKey,
} from "@/features/supplier-settlements/lib/settlement-list-filters"
import {
    STATUS_LABEL,
    type DifferenceType,
} from "@/features/supplier-settlements/types"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

const prefix = "supplier-settlements-list"
const panelId = `${prefix}-more-panel`

const STATUS_FILTER_OPTIONS = SETTLEMENT_STATUS_VALUES.map((value) => ({
    value,
    label: STATUS_LABEL[value],
}))

export function SettlementListToolbar({
    urlState,
    suppliers,
    ownerOptions,
    operatorOptions,
    handlerOptions,
    searchInputRef,
    searchDraft,
    setSearchDraft,
    panelOpen,
    setPanelOpen,
    applyFilters,
    removeFilter,
    resetMoreFilters,
    clearAllFilters,
    supplierIdDraft,
    setSupplierIdDraft,
    statusDraft,
    setStatusDraft,
    differenceTypeDraft,
    setDifferenceTypeDraft,
    periodFromDraft,
    setPeriodFromDraft,
    periodToDraft,
    setPeriodToDraft,
    ownerUserIdsDraft,
    setOwnerUserIdsDraft,
    operatorUserIdsDraft,
    setOperatorUserIdsDraft,
    handlerUserIdsDraft,
    setHandlerUserIdsDraft,
    orgUnitIdsDraft,
    setOrgUnitIdsDraft,
    includeDescendantsDraft,
    setIncludeDescendantsDraft,
    periodError,
    setPeriodError,
    hasPendingChanges,
    resultCount,
    loading,
    failed,
}: {
    urlState: SettlementsUrlState
    suppliers: readonly { supplierId: string; supplierName: string }[]
    ownerOptions: readonly SettlementFilterOption[]
    operatorOptions: readonly SettlementFilterOption[]
    handlerOptions: readonly SettlementFilterOption[]
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: SetState<string>
    panelOpen: boolean
    setPanelOpen: SetState<boolean>
    applyFilters: () => void
    removeFilter: (key: SettlementFilterKey) => void
    resetMoreFilters: () => void
    clearAllFilters: () => void
    supplierIdDraft: string | null
    setSupplierIdDraft: SetState<string | null>
    statusDraft: string[]
    setStatusDraft: SetState<string[]>
    differenceTypeDraft: DifferenceType | "all"
    setDifferenceTypeDraft: SetState<DifferenceType | "all">
    periodFromDraft: string
    setPeriodFromDraft: SetState<string>
    periodToDraft: string
    setPeriodToDraft: SetState<string>
    ownerUserIdsDraft: string
    setOwnerUserIdsDraft: SetState<string>
    operatorUserIdsDraft: string
    setOperatorUserIdsDraft: SetState<string>
    handlerUserIdsDraft: string
    setHandlerUserIdsDraft: SetState<string>
    orgUnitIdsDraft: string
    setOrgUnitIdsDraft: SetState<string>
    includeDescendantsDraft: boolean
    setIncludeDescendantsDraft: SetState<boolean>
    periodError: string | null
    setPeriodError: SetState<string | null>
    hasPendingChanges: boolean
    resultCount?: number
    loading: boolean
    failed: boolean
}) {
    const periodErrorId = `${prefix}-period-error`
    const appliedChips = React.useMemo(
        () => buildSettlementFilterChips(urlState, suppliers),
        [suppliers, urlState],
    )
    const moreCount = appliedChips.filter(({ key }) =>
        [
            "supplierId",
            "status",
            "period",
            "ownerUserIds",
            "operatorUserIds",
            "handlerUserIds",
            "orgUnitIds",
        ].includes(key),
    ).length

    return (
        <ListWorkspaceFilterBar
            density="compact"
            idPrefix={prefix}
            formAriaLabel="结算单查询"
            onSubmit={applyFilters}
            queryButtonId={`${prefix}-filter-apply`}
            moreButtonId={`${prefix}-filter-toggle`}
            resetMoreButtonId={`${prefix}-filter-reset-more`}
            clearButtonId={`${prefix}-filter-clear-all`}
            search={
                <ListSearchField
                    id={`${prefix}-search-input`}
                    searchInputRef={searchInputRef}
                    value={searchDraft}
                    onChange={setSearchDraft}
                    placeholder="结算单号、外部账单号、供应商"
                    aria-label="搜索结算单"
                    data-slot="settlement-list-search"
                />
            }
            moreCount={moreCount}
            moreOpen={panelOpen}
            onToggleMore={() => setPanelOpen((open) => !open)}
            morePanelId={panelId}
            morePanelAriaLabel="结算单列表更多筛选条件"
            onResetMore={resetMoreFilters}
            commonFilters={
                <FixedOptionRadioFilter
                    idPrefix={`${prefix}-filter-difference-type`}
                    label="差异类型"
                    variant="quiet"
                    value={differenceTypeDraft}
                    onValueChange={setDifferenceTypeDraft}
                    options={DIFF_TYPE_RADIO_OPTIONS}
                />
            }
            morePanel={
                <div className="grid min-w-0 gap-5 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_minmax(0,2fr)]">
                    <ResponsibleUserFilter
                        id={`${prefix}-filter-owner`}
                        label="对账负责人"
                        value={ownerUserIdsDraft}
                        onChange={setOwnerUserIdsDraft}
                        options={ownerOptions}
                    />
                    <ResponsibleUserFilter
                        id={`${prefix}-filter-operator`}
                        label="差异处理人"
                        value={operatorUserIdsDraft}
                        onChange={setOperatorUserIdsDraft}
                        options={operatorOptions}
                    />
                    <ResponsibleUserFilter
                        id={`${prefix}-filter-handler`}
                        label="当前复核人"
                        value={handlerUserIdsDraft}
                        onChange={setHandlerUserIdsDraft}
                        options={handlerOptions}
                    />
                    <ListWorkspaceFilterField
                        htmlFor={`${prefix}-filter-org`}
                        label="业务组织"
                    >
                        <Input
                            id={`${prefix}-filter-org`}
                            value={orgUnitIdsDraft}
                            onChange={(event) =>
                                setOrgUnitIdsDraft(event.target.value)
                            }
                            placeholder="组织 ID，逗号分隔"
                            aria-label="按结算业务组织筛选"
                        />
                        <label
                            htmlFor={`${prefix}-filter-org-descendants`}
                            className="mt-2 flex items-center gap-2 text-xs text-muted-foreground"
                        >
                            <Checkbox
                                id={`${prefix}-filter-org-descendants`}
                                checked={includeDescendantsDraft}
                                onCheckedChange={(checked) =>
                                    setIncludeDescendantsDraft(checked === true)
                                }
                            />
                            包含下级
                        </label>
                    </ListWorkspaceFilterField>
                    <ListWorkspaceFilterField
                        htmlFor={`${prefix}-filter-supplier`}
                        label="供应商"
                    >
                        <SupplierSearchCombobox
                            id={`${prefix}-filter-supplier`}
                            purpose="filter"
                            className="w-full"
                            value={supplierIdDraft ?? undefined}
                            onValueChange={(id) =>
                                setSupplierIdDraft(id ?? null)
                            }
                            aria-label="供应商"
                            placeholder="全部供应商"
                        />
                    </ListWorkspaceFilterField>
                    <ListWorkspaceFilterField
                        htmlFor={`${prefix}-filter-status`}
                        label="状态"
                    >
                        <MultiOptionCombobox
                            id={`${prefix}-filter-status`}
                            className="w-full"
                            value={statusDraft}
                            onValueChange={setStatusDraft}
                            options={STATUS_FILTER_OPTIONS}
                            placeholder="全部状态"
                            aria-label="状态"
                        />
                    </ListWorkspaceFilterField>
                    <ListWorkspaceFilterField label="结算期间">
                        <div
                            className="flex items-center gap-1.5"
                            role="group"
                            aria-label="结算期间"
                            aria-describedby={
                                periodError ? periodErrorId : undefined
                            }
                        >
                            <DatePicker
                                id={`${prefix}-filter-period-from`}
                                className="w-0 min-w-0 flex-1"
                                value={periodFromDraft || undefined}
                                onValueChange={(next) => {
                                    setPeriodFromDraft(next ?? "")
                                    setPeriodError(null)
                                }}
                                aria-invalid={Boolean(periodError)}
                                placeholder="期间自"
                            />
                            <span className="text-muted-foreground">至</span>
                            <DatePicker
                                id={`${prefix}-filter-period-to`}
                                className="w-0 min-w-0 flex-1"
                                value={periodToDraft || undefined}
                                onValueChange={(next) => {
                                    setPeriodToDraft(next ?? "")
                                    setPeriodError(null)
                                }}
                                aria-invalid={Boolean(periodError)}
                                placeholder="期间至"
                            />
                        </div>
                        {periodError ? (
                            <span
                                id={periodErrorId}
                                className="text-xs text-destructive"
                                role="alert"
                            >
                                {periodError}
                            </span>
                        ) : null}
                    </ListWorkspaceFilterField>
                </div>
            }
            resultStatus={listWorkspaceFilterStatusText({
                loading,
                failed,
                resultCount,
                noun: "张结算单",
                loadingLabel: "正在加载结算单…",
            })}
            chips={appliedChips}
            onClearChip={(key) => removeFilter(key as SettlementFilterKey)}
            onClearAll={clearAllFilters}
            hasPendingChanges={hasPendingChanges}
            pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
        />
    )
}
