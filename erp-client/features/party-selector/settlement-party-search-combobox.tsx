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
    searchParties,
    fetchPartyOption,
    type PartySelectorPurpose,
} from "./api"
import { getErrorMessage } from "@/lib/api/errors"

const STALE_TIME = 0

export type SettlementPartySearchComboboxProps = Omit<
    SettlementPartyComboboxProps,
    "loading" | "filterMode" | "onSearchChange" | "parties"
> & {
    purpose?: PartySelectorPurpose
    noScopeLabel?: string
    selectedItem?: SettlementPartyComboboxItem
    /** 已授权单据中的结算主体快照；只回显同一已选身份，目录错误仍保留。 */
    selectedSnapshot?: SettlementPartyComboboxItem
    onItemChange?: (item?: SettlementPartyComboboxItem) => void
    /** 已选客户对应主体；空搜索时只列出该主体，输入关键词后仍可搜全部。 */
    restrictToPartyId?: string
}

/** 独立的结算主体公共选择器；不得依赖 master-data feature。 */
export function SettlementPartySearchCombobox({
    purpose = "form",
    selectedItem: _selectedItem,
    selectedSnapshot,
    onItemChange,
    emptyLabel,
    noScopeLabel,
    value,
    onValueChange,
    restrictToPartyId,
    ...props
}: SettlementPartySearchComboboxProps) {
    const [input, setInput] = React.useState("")
    const [query, setQuery] = React.useState("")
    React.useEffect(() => {
        const timer = window.setTimeout(() => setQuery(input.trim()), 250)
        return () => window.clearTimeout(timer)
    }, [input])

    const list = useQuery({
        queryKey: ["party-selector", "list", { purpose, query }],
        queryFn: () => searchParties({ query, purpose }),
        staleTime: STALE_TIME,
    })
    const selected = useQuery({
        queryKey: ["party-selector", "detail", purpose, value ?? ""],
        queryFn: () => fetchPartyOption(value ?? "", purpose),
        enabled: Boolean(value),
        staleTime: STALE_TIME,
    })
    const selectedFailed = Boolean(value) && selected.isError
    const failed = list.isError || selectedFailed
    const error = list.error ?? (selectedFailed ? selected.error : null)
    const snapshot =
        selectedSnapshot?.partyId === value ? selectedSnapshot : undefined
    const selectedRow =
        selected.isError || selected.isFetching
            ? snapshot
            : (selected.data ?? snapshot)
    const rows = [
        ...(list.isError || list.isFetching
            ? []
            : (list.data?.items.filter((item) => item.partyId !== value) ??
              [])),
    ]
    if (
        selectedRow &&
        !rows.some((item) => item.partyId === selectedRow.partyId)
    ) {
        rows.unshift(selectedRow)
    }
    const parties =
        restrictToPartyId && !query
            ? rows.filter((item) => item.partyId === restrictToPartyId)
            : rows

    return (
        <div className="min-w-0">
            <SettlementPartyCombobox
                {...props}
                value={value}
                parties={parties}
                onValueChange={(id) => {
                    onValueChange(id)
                    onItemChange?.(parties.find((item) => item.partyId === id))
                }}
                onSearchChange={setInput}
                filterMode="remote"
                loading={
                    list.isFetching || (selected.isFetching && !selectedRow)
                }
                emptyLabel={
                    failed
                        ? getErrorMessage(error, "结算主体加载失败，请重试")
                        : list.data?.empty_reason === "no_scope"
                          ? (noScopeLabel ??
                            "当前角色无此目录的数据范围，请申请权限")
                          : restrictToPartyId && !query && parties.length === 0
                            ? "请先选择客户"
                            : emptyLabel
                }
            />
            <SelectorQueryFeedback
                noScopeLabel={noScopeLabel}
                id={props.id}
                failed={failed}
                error={error}
                noScope={
                    !list.isFetching &&
                    !list.isError &&
                    list.data?.empty_reason === "no_scope"
                }
                onRetry={() => {
                    void list.refetch()
                    if (value) void selected.refetch()
                }}
            />
        </div>
    )
}
