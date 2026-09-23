"use client"

import type { Dispatch, SetStateAction } from "react"
import { OptionCombobox, OwnerCombobox } from "@/components/business"
import { ListWorkspaceFilterField } from "@/components/business/list-workspace"
import {
    ContractSearchCombobox,
    CustomerSearchCombobox,
} from "@/features/entity-selectors"
import { OrganizationUnitFilter } from "@/features/organization/components/organization-unit-filter"
import {
    SALES_ORDER_CLOSE_OPTIONS,
    SALES_ORDER_COLLECTION_OPTIONS,
    SALES_ORDER_FULFILLMENT_OPTIONS,
    SALES_ORDER_INVOICE_OPTIONS,
    SALES_ORDER_REVIEW_STATUS_OPTIONS,
} from "@/features/sales-orders/lib/filter-orders"
import type { SalesOrdersListFilterDraft } from "@/features/sales-orders/lib/sales-orders-list-filters"
import { useOwnerOptionsQuery } from "@/hooks/use-options"

const progressFields = [
    {
        key: "fulfillment",
        id: "fulfillment",
        label: "履约进度",
        options: SALES_ORDER_FULFILLMENT_OPTIONS,
    },
    {
        key: "collection",
        id: "collection",
        label: "回款进度",
        options: SALES_ORDER_COLLECTION_OPTIONS,
    },
    {
        key: "invoice",
        id: "invoice",
        label: "开票进度",
        options: SALES_ORDER_INVOICE_OPTIONS,
    },
    {
        key: "closeStatus",
        id: "close-status",
        label: "关闭状态",
        options: SALES_ORDER_CLOSE_OPTIONS,
    },
] as const
const sourceFields = [
    {
        key: "origin",
        id: "origin",
        label: "创建来源",
        options: [
            { value: "erp", label: "ERP" },
            { value: "mall", label: "商城" },
        ],
    },
    {
        key: "reviewStatus",
        id: "review-status",
        label: "审核状态",
        options: SALES_ORDER_REVIEW_STATUS_OPTIONS,
    },
] as const

/** 低频条件按用途分组；高频条件由外部查询栏承载。 */
export function SalesOrdersListFilterPanel({
    draft,
    onDraftChange,
}: {
    draft: SalesOrdersListFilterDraft
    onDraftChange: Dispatch<SetStateAction<SalesOrdersListFilterDraft>>
}) {
    const owners = useOwnerOptionsQuery()
    const renderEnum = ({
        key,
        id,
        label,
        options,
    }: {
        key:
            | "fulfillment"
            | "collection"
            | "invoice"
            | "closeStatus"
            | "origin"
            | "reviewStatus"
        id: string
        label: string
        options: readonly { value: string; label: string }[]
    }) => (
        <ListWorkspaceFilterField
            key={key}
            htmlFor={`sales-orders-list-filter-${id}`}
            label={label}
        >
            <OptionCombobox
                id={`sales-orders-list-filter-${id}`}
                className="w-full"
                value={draft[key] === "all" ? null : draft[key]}
                aria-label={label}
                onValueChange={(value) => {
                    if (
                        value !== null &&
                        !options.some((option) => option.value === value)
                    )
                        return
                    onDraftChange((current) => ({
                        ...current,
                        [key]: value ?? "all",
                    }))
                }}
                options={options}
                placeholder="全部"
                searchPlaceholder={`搜索${label}`}
            />
        </ListWorkspaceFilterField>
    )
    return (
        <div className="space-y-5">
            <fieldset className="min-w-0 space-y-3">
                <legend className="mb-1 text-sm font-medium">组织与关联</legend>
                <OrganizationUnitFilter
                    id="sales-orders-list-filter-org"
                    label="业务组织"
                    value={draft.orgUnitIds}
                    onChange={(orgUnitIds) =>
                        onDraftChange((current) => ({ ...current, orgUnitIds }))
                    }
                    includeDescendants={draft.includeDescendants}
                    onDescendantsChange={(includeDescendants) =>
                        onDraftChange((current) => ({
                            ...current,
                            includeDescendants,
                        }))
                    }
                />
                <div className="grid min-w-0 gap-3 sm:grid-cols-2">
                    <ListWorkspaceFilterField
                        htmlFor="sales-orders-list-filter-customer"
                        label="客户"
                    >
                        <CustomerSearchCombobox
                            id="sales-orders-list-filter-customer"
                            purpose="filter"
                            scope="all_authorized"
                            value={draft.customerId || undefined}
                            onValueChange={(customerId) =>
                                onDraftChange((current) => ({
                                    ...current,
                                    customerId: customerId ?? "",
                                    contractId:
                                        customerId === current.customerId
                                            ? current.contractId
                                            : "",
                                }))
                            }
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
                            customerId={draft.customerId || undefined}
                            value={draft.contractId || undefined}
                            onValueChange={(contractId) =>
                                onDraftChange((current) => ({
                                    ...current,
                                    contractId: contractId ?? "",
                                }))
                            }
                            placeholder="全部合同"
                        />
                    </ListWorkspaceFilterField>
                </div>
            </fieldset>
            <fieldset className="min-w-0 border-t pt-4">
                <legend className="pr-2 text-sm font-medium">业务进度</legend>
                <div className="grid gap-3 sm:grid-cols-2">
                    {progressFields.map(renderEnum)}
                </div>
            </fieldset>
            <fieldset className="min-w-0 border-t pt-4">
                <legend className="pr-2 text-sm font-medium">来源与审批</legend>
                <div className="grid gap-3 sm:grid-cols-2">
                    {sourceFields.map(renderEnum)}
                    <ListWorkspaceFilterField
                        htmlFor="sales-orders-list-filter-created-by"
                        label="创建人"
                    >
                        <OwnerCombobox
                            id="sales-orders-list-filter-created-by"
                            owners={owners.data ?? []}
                            loading={owners.isFetching}
                            value={draft.createdBy || undefined}
                            onValueChange={(createdBy) =>
                                onDraftChange((current) => ({
                                    ...current,
                                    createdBy: createdBy ?? "",
                                }))
                            }
                            placeholder="全部创建人"
                        />
                    </ListWorkspaceFilterField>
                </div>
            </fieldset>
        </div>
    )
}
