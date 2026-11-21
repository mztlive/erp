"use client"

import { PermissionGuard } from "@/features/auth/components/permission-guard"

import * as React from "react"

import { MultiOptionCombobox, OptionCombobox } from "@/components/business"
import { PersonDirectoryFilter } from "@/features/entity-selectors/components/person-directory-filter"
import { ResidentSupplierFilter } from "@/features/entity-selectors/components/resident-supplier-filter"
import {
    ListFilterSection,
    ListFilterSections,
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
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

import { listFilterText } from "@/lib/ui-text"

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
    loading: boolean
    failed: boolean
}) {
    const periodErrorId = `${prefix}-period-error`
    const appliedChips = React.useMemo(
        () => buildSettlementFilterChips(urlState, suppliers),
        [suppliers, urlState],
    )
    const showToolbarClear =
        appliedChips.length > 0 &&
        !appliedChips.some(({ key }) => MORE_CHIP_KEYS.includes(key))

    return (
        <ListWorkspaceFilterBar
            density="compact"
            morePresentation="popover"
            moreSize="wide"
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
                        className="w-40 max-w-full min-w-0"
                        filterLabel="状态"
                        aria-label="状态"
                        value={statusDraft}
                        onValueChange={setStatusDraft}
                        options={STATUS_FILTER_OPTIONS}
                        placeholder="全部"
                    />
                    <OptionCombobox
                        id={`${prefix}-filter-difference-type`}
                        className="w-40 max-w-full min-w-0"
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
            extraPrimary={
                showToolbarClear ? (
                    <Button
                        id={`${prefix}-filter-clear-all`}
                        type="button"
                        variant="ghost"
                        size="sm"
                        className="text-muted-foreground"
                        onClick={clearAllFilters}
                    >
                        {listFilterText.clear}
                    </Button>
                ) : null
            }
            morePanel={
                <ListFilterSections>
                    <PermissionGuard permission="business_person:list">
                        <ListFilterSection legend="人员">
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
                        </ListFilterSection>
                    </PermissionGuard>
                    <PermissionGuard permission="org_unit:list">
                        <ListFilterSection legend="组织">
                            <OrganizationUnitFilter
                                id={`${prefix}-filter-org`}
                                label="业务组织"
                                value={orgUnitIdsDraft}
                                onChange={setOrgUnitIdsDraft}
                                includeDescendants={includeDescendantsDraft}
                                onDescendantsChange={setIncludeDescendantsDraft}
                            />
                        </ListFilterSection>
                    </PermissionGuard>
                    <ListFilterSection legend="结算期间">
                        <ListWorkspaceFilterField
                            htmlFor={`${prefix}-filter-period-from`}
                            label="开始日期"
                        >
                            <div data-has-value={Boolean(periodFromDraft)}>
                                <DatePicker
                                    id={`${prefix}-filter-period-from`}
                                    size="sm"
                                    value={periodFromDraft || undefined}
                                    onValueChange={(next) => {
                                        setPeriodFromDraft(next ?? "")
                                        setPeriodError(null)
                                    }}
                                    aria-invalid={Boolean(periodError)}
                                    aria-describedby={
                                        periodError ? periodErrorId : undefined
                                    }
                                    placeholder="期间自"
                                />
                            </div>
                        </ListWorkspaceFilterField>
                        <ListWorkspaceFilterField
                            htmlFor={`${prefix}-filter-period-to`}
                            label="结束日期"
                        >
                            <div data-has-value={Boolean(periodToDraft)}>
                                <DatePicker
                                    id={`${prefix}-filter-period-to`}
                                    size="sm"
                                    value={periodToDraft || undefined}
                                    onValueChange={(next) => {
                                        setPeriodToDraft(next ?? "")
                                        setPeriodError(null)
                                    }}
                                    aria-invalid={Boolean(periodError)}
                                    aria-describedby={
                                        periodError ? periodErrorId : undefined
                                    }
                                    placeholder="期间至"
                                />
                            </div>
                        </ListWorkspaceFilterField>
                        {periodError ? (
                            <span
                                id={periodErrorId}
                                className="col-span-full text-xs text-destructive"
                                role="alert"
                            >
                                {periodError}
                            </span>
                        ) : null}
                    </ListFilterSection>
                </ListFilterSections>
            }
            resultStatus={
                loading || failed
                    ? listWorkspaceFilterStatusText({
                          loading,
                          failed,
                          noun: "张结算单",
                      })
                    : null
            }
            chips={appliedChips}
            foldedChipKeys={MORE_CHIP_KEYS}
            onClearChip={(key) => removeFilter(key as SettlementFilterKey)}
            onClearAll={clearAllFilters}
            hasPendingChanges={hasPendingChanges}
        />
    )
}
