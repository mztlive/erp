"use client"

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
  const eligibility = await Promise.all(
    parties.map(async (party) => {
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
  return eligibility.filter(
    (party): party is SettlementPartyComboboxItem => party != null,
  )
}

/** 只展示已有应收子账的往来主体，避免通用主体搜索扩大核销资格。 */
export function ReceivableCounterpartySearchCombobox({
  purpose = "filter",
  selectedItem,
  onItemChange,
  value,
  onValueChange,
  emptyLabel,
  ...props
}: ReceivableCounterpartySearchComboboxProps) {
  const [input, setInput] = React.useState("")
  const search = useDebouncedSearch(input)
  const list = useQuery({
    queryKey: ["customer-receivables", "counterparty-options", { search, purpose }],
    queryFn: () => searchEligibleCounterparties(search, purpose),
    staleTime: 5 * 60 * 1000,
    placeholderData: (previous) => previous,
  })
  const selected = useQuery({
    queryKey: ["entity-selectors", "party", "detail", value ?? ""],
    queryFn: () => fetchPartyOption(value ?? ""),
    enabled: Boolean(value),
    staleTime: 5 * 60 * 1000,
  })
  const selectedOption = selectedItem ?? selected.data
  const rows = React.useMemo(() => {
    const options = list.data ?? []
    if (
      !selectedOption ||
      options.some((item) => item.partyId === selectedOption.partyId)
    ) {
      return options
    }
    return [selectedOption, ...options]
  }, [list.data, selectedOption])

  return (
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
        list.isError
          ? getErrorMessage(list.error, "往来主体加载失败，请重试")
          : emptyLabel
      }
    />
  )
}
