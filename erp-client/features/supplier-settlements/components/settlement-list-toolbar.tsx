"use client"

import * as React from "react"

import { MultiOptionCombobox, OptionCombobox } from "@/components/business"
import { SelectorQueryFeedback } from "@/components/business/selector-query-feedback"
import { PersonDirectoryFilter } from "@/features/entity-selectors/components/person-directory-filter"
import { useRemoteSearchCombobox } from "@/features/entity-selectors/hooks/use-remote-search-combobox"
import { useSearchInput } from "@/features/entity-selectors/hooks/use-search-input"
import { useSupplierSelectorQuery } from "@/features/entity-selectors/hooks/queries"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { DatePicker } from "@/components/ui/date-picker"
import { OrganizationUnitFilter } from "@/features/organization/components/organization-unit-filter"
import type { SettlementsUrlState } from "@/features/supplier-settlements/lib/url-state"
import {
    buildSettlementFilterChips,
    DIFF_TYPE_FILTER_OPTIONS,
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
const MORE_CHIP_KEYS: readonly SettlementFilterKey[] = [
    "period",
    "ownerUserIds",
    "operatorUserIds",
    "handlerUserIds",
    "orgUnitIds",
]

const STATUS_FILTER_OPTIONS = SETTLEMENT_STATUS_VALUES.map((value) => ({
    value,
    label: STATUS_LABEL[value],
}))

function ResidentSupplierFilter({
    id,
    value,
    onValueChange,
}: {
    id: string
    value: string | null
    onValueChange: (value: string | null) => void
}) {
    const search = useSearchInput()
    const query = useSupplierSelectorQuery(
        { query: search.input, purpose: "filter" },
        value ?? undefined,
    )
    const { rows, loading, emptyLabel } = useRemoteSearchCombobox({
        selectedId: value ?? undefined,
        list: query.list,
        selected: query.selected,
        idOf: (item) => item.supplierId,
        fallbackError: "供应商加载失败，请重试",
    })
    const directoryFailed = query.list.isError || query.selected.isError
    return (
        <div className="min-w-0">
            <OptionCombobox
                id={id}
                className="w-56 max-w-full min-w-0"
                filterLabel="供应商"
                aria-label="供应商"
                placeholder="全部"
                searchPlaceholder="搜索供应商名称或编码"
                filterMode="remote"
                onSearchChange={search.onSearchChange}
                loading={loading}
                emptyLabel={emptyLabel}
                value={value}
                onValueChange={onValueChange}
                options={rows.map((item) => ({
                    value: item.supplierId,
                    label: item.supplierName,
                    keywords: item.supplierCode,
                }))}
            />
            <SelectorQueryFeedback
                id={id}
                failed={directoryFailed}
                error={query.list.error ?? query.selected.error}
                noScope={
                    !query.list.isFetching &&
                    !directoryFailed &&
                    query.list.emptyReason === "no_scope"
                }
                onRetry={() => {
                    void query.list.refetch()
                    if (value) void query.selected.refetch()
                }}
            />
        </div>
    )
}

export function SettlementListToolbar({
    urlState,
    suppliers,

    searchInputRef,
    searchDraft,
    setSearchDraft,
    panelOpen,
    setPanelOpen,
    applyFilters,
    removeFilter,
    resetMoreFilters,
    cancelMoreFilters,
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

    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: SetState<string>
    panelOpen: boolean
    setPanelOpen: SetState<boolean>
    applyFilters: () => void
    removeFilter: (key: SettlementFilterKey) => void
    resetMoreFilters: () => void
    cancelMoreFilters: () => void
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
        MORE_CHIP_KEYS.includes(key),
    ).length

    return (
        <ListWorkspaceFilterBar
            density="compact"
            morePresentation="popover"
            moreSize="wide"
            className="[&_[data-slot=list-toolbar-filters]]:min-w-0 [&_[data-slot=list-toolbar-filters]]:shrink [&_[data-slot=list-toolbar-filters]]:self-center"
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
            onToggleMore={() =>
                panelOpen ? cancelMoreFilters() : setPanelOpen(true)
            }
            morePanelId={panelId}
            morePanelAriaLabel="结算单列表更多筛选条件"
            onResetMore={resetMoreFilters}
            primaryFilters={
                <>
                    <ResidentSupplierFilter
                        id={`${prefix}-filter-supplier`}
                        value={supplierIdDraft}
                        onValueChange={setSupplierIdDraft}
                    />
                    <MultiOptionCombobox
                        id={`${prefix}-filter-status`}
                        className="w-48 max-w-full min-w-0"
                        filterLabel="状态"
                        aria-label="状态"
                        value={statusDraft}
                        onValueChange={setStatusDraft}
                        options={STATUS_FILTER_OPTIONS}
                        placeholder="全部"
                    />
                    <OptionCombobox
                        id={`${prefix}-filter-difference-type`}
                        className="w-52 max-w-full min-w-0"
                        filterLabel="差异类型"
                        aria-label="差异类型"
                        value={
                            differenceTypeDraft === "all"
                                ? null
                                : differenceTypeDraft
                        }
                        options={DIFF_TYPE_FILTER_OPTIONS}
                        placeholder="全部"
                        onValueChange={(value) =>
                            setDifferenceTypeDraft(
                                DIFF_TYPE_FILTER_OPTIONS.some(
                                    (option) => option.value === value,
                                )
                                    ? (value as DifferenceType)
                                    : "all",
                            )
                        }
                    />
                </>
            }
            morePanel={
                <div className="space-y-5">
                    <fieldset className="min-w-0 space-y-3">
                        <legend className="mb-1 text-sm font-medium">
                            人员
                        </legend>
                        <div className="grid min-w-0 gap-3">
                            <PersonDirectoryFilter
                                id={`${prefix}-filter-owner`}
                                label="对账负责人"
                                value={ownerUserIdsDraft}
                                onChange={setOwnerUserIdsDraft}
                                category="business"
                            />
                            <PersonDirectoryFilter
                                id={`${prefix}-filter-operator`}
                                label="差异处理人"
                                value={operatorUserIdsDraft}
                                onChange={setOperatorUserIdsDraft}
                                category="business"
                            />
                            <PersonDirectoryFilter
                                id={`${prefix}-filter-handler`}
                                label="当前复核人"
                                value={handlerUserIdsDraft}
                                onChange={setHandlerUserIdsDraft}
                                category="business"
                            />
                        </div>
                    </fieldset>
                    <fieldset className="min-w-0 border-t pt-4">
                        <legend className="sr-only">业务组织</legend>
                        <OrganizationUnitFilter
                            id={`${prefix}-filter-org`}
                            label="业务组织"
                            value={orgUnitIdsDraft}
                            onChange={setOrgUnitIdsDraft}
                            includeDescendants={includeDescendantsDraft}
                            onDescendantsChange={setIncludeDescendantsDraft}
                        />
                    </fieldset>
                    <fieldset className="min-w-0 border-t pt-4">
                        <legend className="mb-3 text-sm font-medium">
                            结算期间
                        </legend>
                        <div
                            className="flex min-w-0 items-center gap-1.5"
                            role="group"
                            aria-label="结算期间"
                            aria-describedby={
                                periodError ? periodErrorId : undefined
                            }
                        >
                            <DatePicker
                                id={`${prefix}-filter-period-from`}
                                className="w-0 min-w-0 flex-1 [&_button]:h-control"
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
                                className="w-0 min-w-0 flex-1 [&_button]:h-control"
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
                    </fieldset>
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
