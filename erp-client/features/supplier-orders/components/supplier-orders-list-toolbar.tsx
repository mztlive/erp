"use client"

import * as React from "react"

import { MultiOptionCombobox, OptionCombobox } from "@/components/business"
import {
    ListFilterSection,
    ListFilterSections,
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { PersonDirectoryFilter } from "@/features/entity-selectors/components/person-directory-filter"
import { ResidentSupplierFilter } from "@/features/entity-selectors/components/resident-supplier-filter"
import { OrganizationUnitFilter } from "@/features/organization/components/organization-unit-filter"
import type {
    SupplierOrdersFilterKey,
    useSupplierOrdersFilters,
} from "@/features/supplier-orders/hooks/use-supplier-orders-filters"
import type {
    CancelStatus,
    RefundStatus,
    SupplierFulfillmentStatus,
} from "@/features/supplier-orders/types"
import {
    CANCEL_STATUS_LABEL,
    CANCEL_STATUSES,
    FULFILLMENT_STATUS_LABEL,
    FULFILLMENT_STATUSES,
    REFUND_STATUS_LABEL,
    REFUND_STATUSES,
} from "@/features/supplier-orders/types"
import { listFilterText } from "@/lib/ui-text"

const prefix = "supplier-orders-list-filter"
const panelId = `${prefix}-more-panel`
const MORE_CHIP_KEYS: readonly SupplierOrdersFilterKey[] = [
    "cancelStatuses",
    "refundStatuses",
    "aftersalePending",
    "ownerUserIds",
    "handlerUserIds",
    "orgUnitIds",
]

export function SupplierOrdersListToolbar({
    searchInputRef,
    filters: f,
    loading,
    failed,
}: {
    searchInputRef: React.RefObject<HTMLInputElement | null>
    filters: ReturnType<typeof useSupplierOrdersFilters>
    loading: boolean
    failed: boolean
}) {
    const hasFoldedFilters = f.appliedChips.some(({ key }) =>
        MORE_CHIP_KEYS.includes(key),
    )

    return (
        <ListWorkspaceFilterBar
            density="compact"
            morePresentation="popover"
            idPrefix={prefix}
            formAriaLabel="供应商订单查询"
            onSubmit={f.applyFilters}
            search={
                <ListSearchField
                    id="supplier-orders-list-search-input"
                    searchInputRef={searchInputRef}
                    data-slot="sfo-list-search"
                    value={f.searchDraft}
                    onChange={f.setSearchDraft}
                    placeholder="供应商订单号、外部单号"
                    aria-label="搜索供应商订单"
                />
            }
            moreOpen={f.panelOpen}
            onToggleMore={() =>
                f.panelOpen ? f.cancelMoreFilters() : f.setPanelOpen(true)
            }
            morePanelId={panelId}
            morePanelAriaLabel="供应商订单更多筛选条件"
            moreButtonId={`${prefix}-toggle`}
            queryButtonId={`${prefix}-apply`}
            resetMoreButtonId={`${prefix}-reset-more`}
            clearButtonId={`${prefix}-clear-all`}
            onResetMore={f.resetMoreFilters}
            primaryFilters={
                <>
                    {f.canFilterSupplier ? (
                        <ResidentSupplierFilter
                            id={`${prefix}-supplier`}
                            value={f.supplierIdDraft}
                            onValueChange={f.setSupplierIdDraft}
                        />
                    ) : null}
                    <MultiOptionCombobox
                        id={`${prefix}-fulfillment`}
                        className="w-48 max-w-full min-w-0"
                        filterLabel="履约状态"
                        aria-label="履约状态"
                        value={f.fulfillmentStatusesDraft}
                        onValueChange={(values) =>
                            f.setFulfillmentStatusesDraft(
                                values as SupplierFulfillmentStatus[],
                            )
                        }
                        options={FULFILLMENT_STATUSES.map((status) => ({
                            value: status,
                            label: FULFILLMENT_STATUS_LABEL[status],
                        }))}
                        placeholder="全部"
                    />
                </>
            }
            morePanel={
                <ListFilterSections>
                    <ListFilterSection legend="售后">
                        <ListWorkspaceFilterField
                            htmlFor={`${prefix}-cancel`}
                            label="取消状态"
                        >
                            <OptionCombobox
                                id={`${prefix}-cancel`}
                                className="w-full min-w-0"
                                value={f.cancelStatusesDraft[0] ?? ""}
                                onValueChange={(value) =>
                                    f.setCancelStatusesDraft(
                                        value ? [value as CancelStatus] : [],
                                    )
                                }
                                options={CANCEL_STATUSES.map((status) => ({
                                    value: status,
                                    label: CANCEL_STATUS_LABEL[status],
                                }))}
                                aria-label="取消状态"
                                placeholder="全部取消状态"
                            />
                        </ListWorkspaceFilterField>
                        <ListWorkspaceFilterField
                            htmlFor={`${prefix}-aftersale`}
                            label="售后处理"
                        >
                            <OptionCombobox
                                id={`${prefix}-aftersale`}
                                className="w-full min-w-0"
                                value={f.aftersalePendingDraft ? "pending" : ""}
                                onValueChange={(value) =>
                                    f.setAftersalePendingDraft(
                                        value === "pending",
                                    )
                                }
                                options={[
                                    { value: "all", label: "全部" },
                                    {
                                        value: "pending",
                                        label: "售后待处理",
                                    },
                                ]}
                                allowClear={false}
                                placeholder="全部"
                                aria-label="售后处理"
                            />
                        </ListWorkspaceFilterField>
                        <ListWorkspaceFilterField
                            htmlFor={`${prefix}-refund`}
                            label="退款状态"
                        >
                            <OptionCombobox
                                id={`${prefix}-refund`}
                                className="w-full min-w-0"
                                value={f.refundStatusesDraft[0] ?? ""}
                                onValueChange={(value) =>
                                    f.setRefundStatusesDraft(
                                        value ? [value as RefundStatus] : [],
                                    )
                                }
                                options={REFUND_STATUSES.map((status) => ({
                                    value: status,
                                    label: REFUND_STATUS_LABEL[status],
                                }))}
                                aria-label="退款状态"
                                placeholder="全部退款状态"
                            />
                        </ListWorkspaceFilterField>
                    </ListFilterSection>
                    {f.canFilterPeople && (
                        <ListFilterSection legend="人员">
                            <PersonDirectoryFilter
                                id={`${prefix}-owner`}
                                label="跟进人"
                                value={f.ownerUserIdsDraft}
                                onChange={f.setOwnerUserIdsDraft}
                                category="business"
                            />
                            <PersonDirectoryFilter
                                id={`${prefix}-handler`}
                                label="异常处理人"
                                value={f.handlerUserIdsDraft}
                                onChange={f.setHandlerUserIdsDraft}
                                category="business"
                            />
                        </ListFilterSection>
                    )}
                    {f.canFilterOrg && (
                        <ListFilterSection legend="组织">
                            <OrganizationUnitFilter
                                id={`${prefix}-org`}
                                label="业务组织"
                                value={f.orgUnitIdsDraft}
                                onChange={f.setOrgUnitIdsDraft}
                                includeDescendants={f.includeDescendantsDraft}
                                onDescendantsChange={
                                    f.setIncludeDescendantsDraft
                                }
                            />
                        </ListFilterSection>
                    )}
                </ListFilterSections>
            }
            resultStatus={failed ? "查询未完成" : loading ? "查询中…" : null}
            extraPrimary={
                f.hasActiveFilters && !hasFoldedFilters ? (
                    <Button
                        id={`${prefix}-clear-all`}
                        type="button"
                        variant="ghost"
                        size="sm"
                        onClick={f.clearAllFilters}
                    >
                        {listFilterText.clear}
                    </Button>
                ) : null
            }
            chips={f.appliedChips}
            foldedChipKeys={MORE_CHIP_KEYS}
            onClearChip={(key) =>
                f.removeFilter(key as SupplierOrdersFilterKey)
            }
            onClearAll={f.clearAllFilters}
            hasPendingChanges={f.hasPendingChanges}
            pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
        />
    )
}
