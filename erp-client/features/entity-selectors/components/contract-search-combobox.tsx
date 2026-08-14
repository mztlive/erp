"use client"

import {
    ContractCombobox,
    type ContractComboboxItem,
    type ContractComboboxProps,
} from "@/components/business/entity-comboboxes"
import type { SmartProps } from "@/features/entity-selectors/components/types"
import { useContractSelectorQuery } from "@/features/entity-selectors/hooks/queries"
import { useRemoteSearchCombobox } from "@/features/entity-selectors/hooks/use-remote-search-combobox"
import { useSearchInput } from "@/features/entity-selectors/hooks/use-search-input"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"

export type ContractSearchComboboxProps = SmartProps<
    ContractComboboxProps,
    ContractComboboxItem
> & {
    customerId?: string
    selectableOnly?: boolean
}

export function ContractSearchCombobox({
    purpose = "sales-order",
    customerId,
    selectableOnly = false,
    selectedItem,
    onItemChange,
    emptyLabel,
    value,
    onValueChange,
    ...props
}: ContractSearchComboboxProps) {
    const search = useSearchInput()
    const accountProfile = useAccountProfileQuery()
    const canReadAllCustomers = hasPermission(
        accountProfile.data?.permissions,
        "customer_scope:detail",
    )
    const needsAssignedScope = purpose === "sales-order" && !canReadAllCustomers
    const scopeReady = purpose !== "sales-order" || !accountProfile.isPending
    const scope = needsAssignedScope ? "assigned" : undefined
    const query = useContractSelectorQuery(
        {
            query: search.input,
            purpose,
            customerId: customerId || undefined,
            selectableOnly,
            scope,
        },
        value,
        { enabled: scopeReady },
    )
    const { rows, loading, emptyLabel: resolvedEmptyLabel } =
        useRemoteSearchCombobox({
            list: query.list,
            selected: query.selected,
            selectedItem,
            idOf: (item) => item.contractId,
            emptyLabel,
            fallbackError: "合同加载失败，请重试",
            extraLoading: !scopeReady,
        })
    return (
        <ContractCombobox
            {...props}
            value={value}
            contracts={rows}
            onValueChange={(id) => {
                onValueChange(id)
                onItemChange?.(rows.find((item) => item.contractId === id))
            }}
            onSearchChange={search.onSearchChange}
            filterMode="remote"
            loading={loading}
            emptyLabel={resolvedEmptyLabel}
        />
    )
}
