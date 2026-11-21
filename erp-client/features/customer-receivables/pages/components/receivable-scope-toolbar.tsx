"use client"

import { OptionCombobox } from "@/components/business"
import {
    ListFilterSection,
    ListFilterSections,
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { PersonDirectoryFilter } from "@/features/entity-selectors/components/person-directory-filter"
import { OrganizationUnitFilter } from "@/features/organization/components/organization-unit-filter"
import { selectedOrgUnitIds } from "@/features/organization/lib/organization-ids"
import type { useReceivableScopeUrlState } from "@/features/customer-receivables/pages/hooks/use-receivable-scope-url-state"

const prefix = "customer-receivables-scope-toolbar"
const OPERATOR_KIND_OPTIONS = [
    { value: "register", label: "登记" },
    { value: "settle", label: "核销" },
] as const

/** 人员与组织继续走各自的权限目录，已限定的单据仍可从条件带清除。 */
export function ReceivableScopeToolbar({
    urlState,
    loading,
    failed,
    showEmptyClear,
}: {
    urlState: ReturnType<typeof useReceivableScopeUrlState>
    loading: boolean
    failed: boolean
    showEmptyClear: boolean
}) {
    const {
        view,
        canUseOwner,
        canUseBusiness,
        canUseOrg,
        panelOpen,
        appliedChips,
        orgDraft,
        descendantsDraft,
    } = urlState
    const moreCount = appliedChips.filter(
        ({ key }) => key === "operatorUserIds" || key === "orgUnitIds",
    ).length
    const orgUnitIds = selectedOrgUnitIds(orgDraft)

    return (
        <ListWorkspaceFilterBar
            morePresentation="popover"
            idPrefix={prefix}
            formAriaLabel="客户往来范围查询"
            onSubmit={urlState.applyFilters}
            queryButtonId={`${prefix}-apply`}
            moreButtonId={`${prefix}-more-filters`}
            clearButtonId={`${prefix}-clear-all`}
            search={
                <ListSearchField
                    id={`${prefix}-search`}
                    searchInputRef={urlState.searchInputRef}
                    value={urlState.searchDraft}
                    onChange={urlState.setSearchDraft}
                    placeholder="销售单、回款单、发票号"
                    aria-label="搜索客户往来范围"
                />
            }
            moreCount={moreCount}
            moreOpen={panelOpen}
            onToggleMore={() =>
                panelOpen
                    ? urlState.cancelMoreFilters()
                    : urlState.setPanelOpen(true)
            }
            morePanelId={`${prefix}-more-panel`}
            morePanelAriaLabel="客户往来范围更多筛选条件"
            onResetMore={urlState.resetMoreFilters}
            primaryFilters={
                canUseOwner ? (
                    <div className="w-56 max-w-full min-w-0 empty:hidden">
                        <PersonDirectoryFilter
                            id={`${prefix}-sales-owner`}
                            category="sales"
                            label="负责销售"
                            hideLabel
                            value={urlState.salesOwnerDraft}
                            onChange={urlState.setSalesOwnerDraft}
                            orgUnitIds={orgUnitIds}
                            includeDescendants={descendantsDraft}
                        />
                    </div>
                ) : null
            }
            morePanel={
                canUseBusiness || canUseOrg ? (
                    <ListFilterSections>
                        {canUseBusiness ? (
                            <ListFilterSection legend="人员">
                                <PersonDirectoryFilter
                                    id={`${prefix}-operator`}
                                    category="business"
                                    label={
                                        view === "receipt"
                                            ? "登记/核销经办人"
                                            : "登记经办人"
                                    }
                                    value={urlState.operatorDraft}
                                    onChange={urlState.setOperatorDraft}
                                    orgUnitIds={orgUnitIds}
                                    includeDescendants={descendantsDraft}
                                />
                                {view === "receipt" ? (
                                    <ListWorkspaceFilterField
                                        htmlFor={`${prefix}-operator-kind`}
                                        label="经办人口径"
                                    >
                                        <OptionCombobox
                                            id={`${prefix}-operator-kind`}
                                            className="w-full min-w-0"
                                            aria-label="经办人口径"
                                            allowClear={false}
                                            value={
                                                urlState.operatorKindDraft ||
                                                "register"
                                            }
                                            options={OPERATOR_KIND_OPTIONS}
                                            onValueChange={(value) => {
                                                if (
                                                    value === "register" ||
                                                    value === "settle"
                                                ) {
                                                    urlState.setOperatorKindDraft(
                                                        value,
                                                    )
                                                }
                                            }}
                                        />
                                    </ListWorkspaceFilterField>
                                ) : null}
                            </ListFilterSection>
                        ) : null}
                        {canUseOrg ? (
                            <ListFilterSection legend="组织">
                                <OrganizationUnitFilter
                                    compact
                                    id={`${prefix}-org`}
                                    label="业务组织"
                                    value={orgDraft}
                                    onChange={urlState.setOrgDraft}
                                    includeDescendants={descendantsDraft}
                                    onDescendantsChange={
                                        urlState.setDescendantsDraft
                                    }
                                />
                            </ListFilterSection>
                        ) : null}
                    </ListFilterSections>
                ) : null
            }
            extraPrimary={
                showEmptyClear ? (
                    <Button
                        id="customer-receivables-scope-banner-clear-filters"
                        type="button"
                        variant="ghost"
                        size="sm"
                        onClick={urlState.clearFilters}
                    >
                        清除筛选
                    </Button>
                ) : null
            }
            resultStatus={failed ? "查询未完成" : loading ? "查询中…" : null}
            chips={appliedChips.map(({ key, label }) => ({
                key,
                label,
                onClear: () => urlState.removeFilter(key),
            }))}
            onClearAll={urlState.clearFilters}
            hasPendingChanges={urlState.hasPendingChanges}
            pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
        />
    )
}
