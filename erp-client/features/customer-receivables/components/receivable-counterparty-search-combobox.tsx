"use client"

import { SelectorQueryFeedback } from "@/components/business/selector-query-feedback"

import * as React from "react"
import { useQuery } from "@tanstack/react-query"

import {
    SettlementPartyCombobox,
    type SettlementPartyComboboxItem,
    type SettlementPartyComboboxProps,
} from "@/components/business/entity-comboboxes"
import {
    fetchPartyOption,
    searchParties,
    useDebouncedSearch,
} from "@/features/entity-selectors"
import { apiGet } from "@/lib/api"
import { getErrorMessage } from "@/lib/api/errors"
import type { Page } from "@/lib/api/paging"

type ReceivableAccountIdentity = Readonly<{ id: string }>

export type ReceivableCounterpartySearchComboboxProps = Omit<
    SettlementPartyComboboxProps,
    "parties" | "loading" | "filterMode" | "onSearchChange"
> & {
    purpose?: "filter" | "form"
    selectedItem?: SettlementPartyComboboxItem
    onItemChange?: (item?: SettlementPartyComboboxItem) => void
}

async function searchEligibleCounterparties(
    query: string,
    purpose: "filter" | "form",
) {
    const parties = await searchParties({ query, purpose })
    if (purpose === "filter") return parties
    const eligibility = await Promise.all(
        parties.items.map(async (party) => {
            const page = await apiGet<Page<ReceivableAccountIdentity>>(
                "/admin/receivable-accounts",
                {
                    counterparty_party_id: party.partyId,
                    page: 1,
                    page_size: 1,
                },
            )
            return page.total > 0 ? party : null
        }),
    )
    return {
        ...parties,
        items: eligibility.filter(
            (party): party is SettlementPartyComboboxItem => party != null,
        ),
    }
}

/** 只展示已有应收子账的往来主体，避免通用主体搜索扩大核销资格。 */
export function ReceivableCounterpartySearchCombobox({
    purpose = "filter",
    selectedItem: _selectedItem,
    onItemChange,
    value,
    onValueChange,
    emptyLabel,
    ...props
}: ReceivableCounterpartySearchComboboxProps) {
    const [input, setInput] = React.useState("")
    const search = useDebouncedSearch(input)
    const list = useQuery({
        queryKey: [
            "customer-receivables",
            "counterparty-options",
            { search, purpose },
        ],
        queryFn: () => searchEligibleCounterparties(search, purpose),
        staleTime: 0,
    })
    const selected = useQuery({
        queryKey: [
            "customer-receivables",
            "counterparty-selected",
            purpose,
            value ?? "",
        ],
        queryFn: async () => {
            if (purpose === "filter")
                return fetchPartyOption(value ?? "", purpose)
            const options = await searchEligibleCounterparties("", purpose)
            return options.items.find((item) => item.partyId === value) ?? null
        },
        enabled: Boolean(value),
        staleTime: 0,
    })
    const selectedOption =
        selected.isError || selected.isFetching ? undefined : selected.data
    const rows = React.useMemo(() => {
        const options = list.isError || list.isFetching ? [] : (list.data?.items.filter((item) => item.partyId !== value) ?? [])
        if (
            !selectedOption ||
            options.some((item) => item.partyId === selectedOption.partyId)
        ) {
            return options
        }
        return [selectedOption, ...options]
    }, [list.data, list.isError, list.isFetching, selectedOption, value])

    return (
        <div className="min-w-0">
            <SettlementPartyCombobox
                {...props}
                value={value}
                onValueChange={(id) => {
                    onValueChange(id)
                    onItemChange?.(rows.find((item) => item.partyId === id))
                }}
                parties={rows}
                onSearchChange={setInput}
                filterMode="remote"
                loading={list.isFetching || selected.isFetching}
                emptyLabel={
                    list.isError || selected.isError
                        ? getErrorMessage(
                              list.error ?? selected.error,
                              "往来主体加载失败，请重试",
                          )
                        : list.data?.empty_reason === "no_scope"
                          ? "当前角色无此目录的数据范围，请申请权限"
                          : emptyLabel
                }
            />
            <SelectorQueryFeedback
                id={props.id}
                failed={list.isError || selected.isError}
                error={list.error ?? selected.error}
                noScope={!list.isFetching && !list.isError && list.data?.empty_reason === "no_scope"}
                onRetry={() => {
                    void list.refetch()
                    if (value) void selected.refetch()
                }}
            />
        </div>
    )
}
