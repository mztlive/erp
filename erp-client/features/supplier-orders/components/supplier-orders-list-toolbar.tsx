"use client"

import * as React from "react"

import { MultiOptionCombobox, OptionCombobox } from "@/components/business"
import { SelectorQueryFeedback } from "@/components/business/selector-query-feedback"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { DatePicker } from "@/components/ui/date-picker"
import { useRemoteSearchCombobox } from "@/features/entity-selectors/hooks/use-remote-search-combobox"
import { useSearchInput } from "@/features/entity-selectors/hooks/use-search-input"
import { useSupplierSelectorQuery } from "@/features/entity-selectors/hooks/queries"
import { PersonDirectoryFilter } from "@/features/entity-selectors/components/person-directory-filter"
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

const prefix = "supplier-orders-list-filter"
const panelId = `${prefix}-more-panel`
const paidErrorId = `${prefix}-paid-error`
const MORE_CHIP_KEYS: readonly SupplierOrdersFilterKey[] = [
    "cancelStatuses",
    "refundStatuses",
    "paidRange",
    "aftersalePending",
    "ownerUserIds",
    "handlerUserIds",
    "orgUnitIds",
]

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

export function SupplierOrdersListToolbar({
    searchInputRef,
    filters: f,
    resultCount,
    loading,
    failed,
}: {
    searchInputRef: React.RefObject<HTMLInputElement | null>
    filters: ReturnType<typeof useSupplierOrdersFilters>
    resultCount?: number
    loading: boolean
    failed: boolean
}) {
    const moreCount = f.appliedChips.filter(({ key }) =>
        MORE_CHIP_KEYS.includes(key),
    ).length

    return (
        <ListWorkspaceFilterBar
            density="compact"
            morePresentation="popover"
            moreSize="wide"
            className="[&_[data-slot=list-toolbar-filters]]:min-w-0 [&_[data-slot=list-toolbar-filters]]:shrink [&_[data-slot=list-toolbar-filters]]:self-center"
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
            moreCount={moreCount}
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
                    <ResidentSupplierFilter
                        id={`${prefix}-supplier`}
                        value={f.supplierIdDraft}
                        onValueChange={f.setSupplierIdDraft}
                    />
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
                <div className="space-y-5">
                    <fieldset className="min-w-0 space-y-3">
                        <legend className="mb-1 text-sm font-medium">
                            售后
                        </legend>
                        <div className="grid min-w-0 gap-3 sm:grid-cols-2">
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
                                            value
                                                ? [value as CancelStatus]
                                                : [],
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
                                    value={
                                        f.aftersalePendingDraft
                                            ? "pending"
                                            : "all"
                                    }
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
                                            value
                                                ? [value as RefundStatus]
                                                : [],
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
                        </div>
                    </fieldset>
                    <fieldset className="min-w-0 space-y-3 border-t pt-4">
                        <legend className="pr-2 text-sm font-medium">
                            人员
                        </legend>
                        <div className="grid min-w-0 gap-3">
                            {/* 未接人员目录：跟进人不是采购查询资格，处理人是开放异常任务负责人。 */}
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
                        </div>
                    </fieldset>
                    <fieldset className="min-w-0 border-t pt-4">
                        <legend className="sr-only">业务组织</legend>
                        <OrganizationUnitFilter
                            id={`${prefix}-org`}
                            label="业务组织"
                            value={f.orgUnitIdsDraft}
                            onChange={f.setOrgUnitIdsDraft}
                            includeDescendants={f.includeDescendantsDraft}
                            onDescendantsChange={f.setIncludeDescendantsDraft}
                        />
                    </fieldset>
                    <fieldset className="min-w-0 border-t pt-4">
                        <legend className="mb-3 text-sm font-medium">
                            支付日期
                        </legend>
                        <div className="flex min-w-0 items-center gap-1.5">
                            <DatePicker
                                id={`${prefix}-paid-from`}
                                className="w-0 min-w-0 flex-1 [&_button]:h-control"
                                value={f.paidFromDraft || undefined}
                                onValueChange={(next) => {
                                    f.setPaidFromDraft(next ?? "")
                                    f.setFilterError(null)
                                }}
                                placeholder="开始日期"
                                aria-invalid={Boolean(f.filterError)}
                                aria-describedby={
                                    f.filterError ? paidErrorId : undefined
                                }
                            />
                            <span className="text-muted-foreground">至</span>
                            <DatePicker
                                id={`${prefix}-paid-to`}
                                className="w-0 min-w-0 flex-1 [&_button]:h-control"
                                value={f.paidToDraft || undefined}
                                onValueChange={(next) => {
                                    f.setPaidToDraft(next ?? "")
                                    f.setFilterError(null)
                                }}
                                placeholder="结束日期"
                                aria-invalid={Boolean(f.filterError)}
                                aria-describedby={
                                    f.filterError ? paidErrorId : undefined
                                }
                            />
                        </div>
                        {f.filterError ? (
                            <span
                                id={paidErrorId}
                                className="text-xs text-destructive"
                                role="alert"
                            >
                                {f.filterError}
                            </span>
                        ) : null}
                    </fieldset>
                </div>
            }
            resultStatus={listWorkspaceFilterStatusText({
                loading,
                failed,
                resultCount,
                noun: "笔供应商订单",
                loadingLabel: "正在加载供应商订单…",
            })}
            chips={f.appliedChips}
            onClearChip={(key) =>
                f.removeFilter(key as SupplierOrdersFilterKey)
            }
            onClearAll={f.clearAllFilters}
            hasPendingChanges={f.hasPendingChanges}
            pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
        />
    )
}
