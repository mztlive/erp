"use client"

import { SelectorQueryFeedback } from "@/components/business/selector-query-feedback"

import {
    CustomerCombobox,
    type CustomerComboboxItem,
    type CustomerComboboxProps,
} from "@/components/business/entity-comboboxes"
import type { SmartProps } from "@/features/entity-selectors/components/types"
import { useCustomerSelectorQuery } from "@/features/entity-selectors/hooks/queries"
import { useRemoteSearchCombobox } from "@/features/entity-selectors/hooks/use-remote-search-combobox"
import { useSearchInput } from "@/features/entity-selectors/hooks/use-search-input"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"

export type CustomerSearchComboboxProps = SmartProps<
    CustomerComboboxProps,
    CustomerComboboxItem
> & {
    scope?: "mine" | "collaborating" | "assigned" | "all_authorized"
}

export function CustomerSearchCombobox({
    purpose = "form",
    scope,
    selectedItem,
    onItemChange,
    emptyLabel,
    value,
    onValueChange,
    ...props
}: CustomerSearchComboboxProps) {
    const search = useSearchInput()
    const profile = useAccountProfileQuery()
    const canReadAll = hasPermission(
        profile.data?.permissions,
        "customer_scope:detail",
    )
    const query = useCustomerSelectorQuery(
        {
            query: search.input,
            purpose,
            scope:
                scope ??
                (purpose === "filter" && canReadAll
                    ? "all_authorized"
                    : "assigned"),
        },
        value,
        { enabled: profile.isSuccess && !profile.isFetching },
    )
    const {
        rows,
        loading,
        emptyLabel: resolvedEmptyLabel,
    } = useRemoteSearchCombobox({
        selectedId: value,
        list: query.list,
        selected: query.selected,
        selectedItem,
        idOf: (item) => item.id,
        emptyLabel,
        fallbackError: "客户加载失败，请重试",
        extraLoading: profile.isFetching,
        blocked: !profile.isSuccess,
    })
    return (
        <div className="min-w-0">
            <CustomerCombobox
                {...props}
                value={value}
                customers={rows}
                onValueChange={(id) => {
                    onValueChange(id)
                    onItemChange?.(rows.find((item) => item.id === id))
                }}
                onSearchChange={search.onSearchChange}
                filterMode="remote"
                loading={loading}
                emptyLabel={resolvedEmptyLabel}
            />
            <SelectorQueryFeedback
                id={props.id}
                failed={
                    profile.isError ||
                    query.list.isError ||
                    query.selected.isError
                }
                error={
                    profile.error ?? query.list.error ?? query.selected.error
                }
                noScope={
                    !profile.isError &&
                    !query.list.isFetching &&
                    !query.list.isError &&
                    query.list.emptyReason === "no_scope"
                }
                onRetry={() => {
                    if (profile.isError) {
                        void profile.refetch()
                    } else {
                        void query.list.refetch()
                        if (value) void query.selected.refetch()
                    }
                }}
            />
        </div>
    )
}
