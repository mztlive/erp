"use client"

import * as React from "react"

import { MultiOptionCombobox, OptionCombobox } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    ListWorkspaceInlineFilter,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { DatePicker } from "@/components/ui/date-picker"
import { SupplierSearchCombobox } from "@/features/entity-selectors"
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
]

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
            onToggleMore={() => f.setPanelOpen((open) => !open)}
            morePanelId={panelId}
            morePanelAriaLabel="供应商订单更多筛选条件"
            moreButtonId={`${prefix}-toggle`}
            queryButtonId={`${prefix}-apply`}
            resetMoreButtonId={`${prefix}-reset-more`}
            clearButtonId={`${prefix}-clear-all`}
            onResetMore={f.resetMoreFilters}
            commonFilters={
                <>
                    <ListWorkspaceInlineFilter
                        htmlFor={`${prefix}-supplier`}
                        label="供应商"
                    >
                        <SupplierSearchCombobox
                            id={`${prefix}-supplier`}
                            className="w-full sm:w-60"
                            purpose="filter"
                            value={f.supplierIdDraft ?? undefined}
                            onValueChange={(id) =>
                                f.setSupplierIdDraft(id ?? null)
                            }
                            placeholder="全部供应商"
                        />
                    </ListWorkspaceInlineFilter>
                    <ListWorkspaceInlineFilter
                        htmlFor={`${prefix}-fulfillment`}
                        label="履约状态"
                    >
                        <MultiOptionCombobox
                            id={`${prefix}-fulfillment`}
                            className="w-full sm:w-60"
                            value={f.fulfillmentStatusesDraft}
                            onValueChange={(values) =>
                                f.setFulfillmentStatusesDraft(
                                    values as SupplierFulfillmentStatus[],
                                )
                            }
                            options={FULFILLMENT_STATUSES.map((s) => ({
                                value: s,
                                label: FULFILLMENT_STATUS_LABEL[s],
                            }))}
                            aria-label="履约状态"
                            placeholder="全部履约状态"
                        />
                    </ListWorkspaceInlineFilter>
                </>
            }
            morePanel={
                <div className="grid min-w-0 gap-5 lg:grid-cols-2">
                    <fieldset className="min-w-0">
                        <legend className="mb-3 text-xs font-medium">
                            取消与退款
                        </legend>
                        <div className="grid min-w-0 gap-3 sm:grid-cols-2">
                            <ListWorkspaceFilterField
                                htmlFor={`${prefix}-cancel`}
                                label="取消状态"
                            >
                                <OptionCombobox
                                    id={`${prefix}-cancel`}
                                    className="w-full"
                                    value={f.cancelStatusesDraft[0] ?? ""}
                                    onValueChange={(value) =>
                                        f.setCancelStatusesDraft(
                                            value
                                                ? [value as CancelStatus]
                                                : [],
                                        )
                                    }
                                    options={CANCEL_STATUSES.map((s) => ({
                                        value: s,
                                        label: CANCEL_STATUS_LABEL[s],
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
                                    className="w-full"
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
                                    className="w-full"
                                    value={f.refundStatusesDraft[0] ?? ""}
                                    onValueChange={(value) =>
                                        f.setRefundStatusesDraft(
                                            value
                                                ? [value as RefundStatus]
                                                : [],
                                        )
                                    }
                                    options={REFUND_STATUSES.map((s) => ({
                                        value: s,
                                        label: REFUND_STATUS_LABEL[s],
                                    }))}
                                    aria-label="退款状态"
                                    placeholder="全部退款状态"
                                />
                            </ListWorkspaceFilterField>
                        </div>
                    </fieldset>
                    <fieldset className="min-w-0 lg:border-l lg:pl-5">
                        <legend className="mb-3 text-xs font-medium">
                            支付时间
                        </legend>
                        <ListWorkspaceFilterField label="支付日期">
                            <div className="flex min-w-0 items-center gap-1.5">
                                <DatePicker
                                    id={`${prefix}-paid-from`}
                                    className="w-0 min-w-0 flex-1"
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
                                <span className="text-muted-foreground">
                                    至
                                </span>
                                <DatePicker
                                    id={`${prefix}-paid-to`}
                                    className="w-0 min-w-0 flex-1"
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
                        </ListWorkspaceFilterField>
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
