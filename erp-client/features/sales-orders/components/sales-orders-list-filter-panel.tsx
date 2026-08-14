"use client"

import * as React from "react"
import { SearchIcon } from "lucide-react"

import { FixedOptionRadioFilter } from "@/components/business"
import { OwnerCombobox } from "@/components/business"
import { Button } from "@/components/ui/button"
import { DateRangePicker } from "@/components/ui/date-picker"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
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
} from "@/features/sales-orders/lib/filter-orders"
import type { SalesOrdersListFilterDraft } from "@/features/sales-orders/lib/sales-orders-list-filters"
import { useOwnerOptionsQuery } from "@/hooks/use-options"

export function SalesOrdersListFilterPanel(props: {
    draft: SalesOrdersListFilterDraft
    onDraftChange: React.Dispatch<React.SetStateAction<SalesOrdersListFilterDraft>>
}) {
    const { draft: filterDraft, onDraftChange: setFilterDraft } = props
    const ownerOptionsQuery = useOwnerOptionsQuery()

    return (
        <div
            id="sales-order-filter-panel"
            className="flex w-full flex-col gap-4 rounded-lg border border-border/60 bg-muted/30 px-3 py-3"
            aria-label="销售单筛选条件"
        >
            <FixedOptionRadioFilter
                label="业务性质"
                value={filterDraft.nature}
                onValueChange={(nature) => {
                    setFilterDraft((draft) => ({
                        ...draft,
                        nature,
                    }))
                }}
                options={[
                    { value: "all", label: "全部" },
                    {
                        value: "physical_service",
                        label: "实物与服务",
                    },
                    {
                        value: "card_voucher",
                        label: "卡券",
                    },
                ]}
            />
            <FixedOptionRadioFilter
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
                label="审核状态"
                value={filterDraft.reviewStatus}
                onValueChange={(reviewStatus) => {
                    setFilterDraft((draft) => ({
                        ...draft,
                        reviewStatus,
                    }))
                }}
                options={[
                    { value: "all", label: "全部" },
                    ...SALES_ORDER_REVIEW_STATUS_OPTIONS,
                ]}
            />
            <FixedOptionRadioFilter
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

            <FieldGroup className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-4">
                <Field>
                    <FieldLabel>客户</FieldLabel>
                    <CustomerSearchCombobox
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
                </Field>
                <Field>
                    <FieldLabel>合同</FieldLabel>
                    <ContractSearchCombobox
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
                </Field>
                <Field>
                    <FieldLabel>创建人</FieldLabel>
                    <OwnerCombobox
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
                </Field>
                <Field>
                    <FieldLabel>创建日期</FieldLabel>
                    <DateRangePicker
                        className="w-full"
                        value={
                            filterDraft.createdFrom || filterDraft.createdTo
                                ? {
                                      from: filterDraft.createdFrom || undefined,
                                      to: filterDraft.createdTo || undefined,
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
                </Field>
            </FieldGroup>

            <div className="flex justify-end">
                <Button type="submit" size="sm">
                    <SearchIcon data-icon="inline-start" aria-hidden="true" />
                    搜索
                </Button>
            </div>
        </div>
    )
}
