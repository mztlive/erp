"use client"

import {
    ResponsibleUserFilter,
    type ResponsibleUserOption,
} from "@/features/entity-selectors/components/responsible-user-filter"

import * as React from "react"

import {
    FixedOptionRadioFilter,
    OptionCombobox,
    OwnerCombobox,
} from "@/components/business"
import { ListWorkspaceFilterField } from "@/components/business/list-workspace"
import { DateRangePicker } from "@/components/ui/date-picker"
import {
    ContractSearchCombobox,
    CustomerSearchCombobox,
} from "@/features/entity-selectors"
import {
    SALES_ORDER_CLOSE_OPTIONS,
    SALES_ORDER_COLLECTION_OPTIONS,
    SALES_ORDER_COMMERCIAL_STATUS_OPTIONS,
    SALES_ORDER_FULFILLMENT_OPTIONS,
    SALES_ORDER_INVOICE_OPTIONS,
    SALES_ORDER_REVIEW_STATUS_OPTIONS,
    type SalesOrderReviewStatusFilter,
} from "@/features/sales-orders/lib/filter-orders"
import type { SalesOrdersListFilterDraft } from "@/features/sales-orders/lib/sales-orders-list-filters"
import { useOwnerOptionsQuery } from "@/hooks/use-options"

export function SalesOrdersListFilterPanel(props: {
    ownerOptions: readonly ResponsibleUserOption[]
    draft: SalesOrdersListFilterDraft
    onDraftChange: React.Dispatch<
        React.SetStateAction<SalesOrdersListFilterDraft>
    >
}) {
    const { draft: filterDraft, onDraftChange: setFilterDraft } = props
    const ownerOptionsQuery = useOwnerOptionsQuery()

    return (
        <div className="grid min-w-0 gap-5">
            <ResponsibleUserFilter
                id="sales-orders-list-owner"
                label="负责销售"
                value={filterDraft.ownerUserIds}
                onChange={(ownerUserIds) =>
                    setFilterDraft((draft) => ({ ...draft, ownerUserIds }))
                }
                options={props.ownerOptions}
            />
            <fieldset className="min-w-0">
                <legend className="mb-3 text-xs font-medium">来源与状态</legend>
                <div className="grid min-w-0 gap-3">
                    <FixedOptionRadioFilter
                        id="sales-orders-list-filter-origin"
                        label="创建来源"
                        value={filterDraft.origin}
                        onValueChange={(origin) => {
                            setFilterDraft((draft) => ({
                                ...draft,
                                origin,
                            }))
                        }}
                        options={[
                            { value: "all", label: "全部" },
                            { value: "erp", label: "ERP" },
                            {
                                value: "mall",
                                label: "商城",
                            },
                        ]}
                    />
                    <FixedOptionRadioFilter
                        id="sales-orders-list-filter-commercial-status"
                        label="商业状态"
                        value={filterDraft.commercialStatus}
                        onValueChange={(commercialStatus) => {
                            setFilterDraft((draft) => ({
                                ...draft,
                                commercialStatus,
                            }))
                        }}
                        options={[
                            { value: "all", label: "全部" },
                            ...SALES_ORDER_COMMERCIAL_STATUS_OPTIONS,
                        ]}
                    />
                    <FixedOptionRadioFilter
                        id="sales-orders-list-filter-fulfillment"
                        label="履约进度"
                        value={filterDraft.fulfillment}
                        onValueChange={(fulfillment) => {
                            setFilterDraft((draft) => ({
                                ...draft,
                                fulfillment,
                            }))
                        }}
                        options={[
                            { value: "all", label: "全部" },
                            ...SALES_ORDER_FULFILLMENT_OPTIONS,
                        ]}
                    />
                    <FixedOptionRadioFilter
                        id="sales-orders-list-filter-collection"
                        label="回款进度"
                        value={filterDraft.collection}
                        onValueChange={(collection) => {
                            setFilterDraft((draft) => ({
                                ...draft,
                                collection,
                            }))
                        }}
                        options={[
                            { value: "all", label: "全部" },
                            ...SALES_ORDER_COLLECTION_OPTIONS,
                        ]}
                    />
                    <FixedOptionRadioFilter
                        id="sales-orders-list-filter-invoice"
                        label="开票进度"
                        value={filterDraft.invoice}
                        onValueChange={(invoice) => {
                            setFilterDraft((draft) => ({
                                ...draft,
                                invoice,
                            }))
                        }}
                        options={[
                            { value: "all", label: "全部" },
                            ...SALES_ORDER_INVOICE_OPTIONS,
                        ]}
                    />
                    <FixedOptionRadioFilter
                        id="sales-orders-list-filter-close-status"
                        label="关闭状态"
                        value={filterDraft.closeStatus}
                        onValueChange={(closeStatus) => {
                            setFilterDraft((draft) => ({
                                ...draft,
                                closeStatus,
                            }))
                        }}
                        options={[
                            { value: "all", label: "全部" },
                            ...SALES_ORDER_CLOSE_OPTIONS,
                        ]}
                    />
                </div>
            </fieldset>
            <fieldset className="min-w-0">
                <legend className="mb-3 text-xs font-medium">关联与日期</legend>
                <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
                    <ListWorkspaceFilterField
                        htmlFor="sales-orders-list-filter-customer"
                        label="客户"
                    >
                        <CustomerSearchCombobox
                            id="sales-orders-list-filter-customer"
                            purpose="filter"
                            scope="all_authorized"
                            value={filterDraft.customerId || undefined}
                            onValueChange={(customerId) => {
                                setFilterDraft((draft) => ({
                                    ...draft,
                                    customerId: customerId ?? "",
                                    contractId:
                                        customerId === draft.customerId
                                            ? draft.contractId
                                            : "",
                                }))
                            }}
                            placeholder="全部客户"
                        />
                    </ListWorkspaceFilterField>
                    <ListWorkspaceFilterField
                        htmlFor="sales-orders-list-filter-contract"
                        label="合同"
                    >
                        <ContractSearchCombobox
                            id="sales-orders-list-filter-contract"
                            purpose="filter"
                            customerId={filterDraft.customerId || undefined}
                            value={filterDraft.contractId || undefined}
                            onValueChange={(contractId) => {
                                setFilterDraft((draft) => ({
                                    ...draft,
                                    contractId: contractId ?? "",
                                }))
                            }}
                            placeholder="全部合同"
                        />
                    </ListWorkspaceFilterField>
                    <ListWorkspaceFilterField
                        htmlFor="sales-orders-list-filter-created-by"
                        label="创建人"
                    >
                        <OwnerCombobox
                            id="sales-orders-list-filter-created-by"
                            owners={ownerOptionsQuery.data ?? []}
                            loading={ownerOptionsQuery.isFetching}
                            value={filterDraft.createdBy || undefined}
                            onValueChange={(createdBy) => {
                                setFilterDraft((draft) => ({
                                    ...draft,
                                    createdBy: createdBy ?? "",
                                }))
                            }}
                            placeholder="全部创建人"
                        />
                    </ListWorkspaceFilterField>
                    <ListWorkspaceFilterField
                        htmlFor="sales-orders-list-filter-created-date"
                        label="创建日期"
                    >
                        <DateRangePicker
                            id="sales-orders-list-filter-created-date"
                            className="w-full"
                            value={
                                filterDraft.createdFrom || filterDraft.createdTo
                                    ? {
                                          from:
                                              filterDraft.createdFrom ||
                                              undefined,
                                          to:
                                              filterDraft.createdTo ||
                                              undefined,
                                      }
                                    : undefined
                            }
                            onValueChange={(range) => {
                                setFilterDraft((draft) => ({
                                    ...draft,
                                    createdFrom: range?.from ?? "",
                                    createdTo: range?.to ?? "",
                                }))
                            }}
                            placeholder="全部日期"
                        />
                    </ListWorkspaceFilterField>
                    <ListWorkspaceFilterField
                        htmlFor="sales-orders-list-filter-review-status"
                        label="审核状态"
                    >
                        <OptionCombobox
                            id="sales-orders-list-filter-review-status"
                            className="w-full"
                            value={
                                filterDraft.reviewStatus === "all"
                                    ? null
                                    : filterDraft.reviewStatus
                            }
                            aria-label="审核状态"
                            onValueChange={(reviewStatus) => {
                                setFilterDraft((draft) => ({
                                    ...draft,
                                    reviewStatus: (reviewStatus ??
                                        "all") as SalesOrderReviewStatusFilter,
                                }))
                            }}
                            options={SALES_ORDER_REVIEW_STATUS_OPTIONS}
                            placeholder="全部审核状态"
                            searchPlaceholder="搜索审核状态"
                        />
                    </ListWorkspaceFilterField>
                </div>
            </fieldset>
        </div>
    )
}
