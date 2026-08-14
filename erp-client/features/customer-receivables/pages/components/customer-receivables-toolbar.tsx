"use client"

import * as React from "react"
import { RefreshCwIcon, SearchIcon } from "lucide-react"

import { FilterChip, ListToolbar, OptionCombobox } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { ReceivableCounterpartySearchCombobox } from "@/features/customer-receivables/components/receivable-counterparty-search-combobox"
import {
    DUE_LABEL,
    type CustomerAccountsView,
    type DueFilter,
} from "@/features/customer-receivables/types"
import type { CustomerReceivablesPatchUrl } from "../hooks/use-customer-receivables-url-state"

type CustomerReceivablesToolbarProps = {
    view: CustomerAccountsView
    due: DueFilter | undefined
    status: string | undefined
    reviewStatus: string | undefined
    counterpartyPartyId: string | undefined
    customerId: string | undefined
    lockedCustomerName: string | undefined
    hasActiveFilters: boolean
    total: number
    searchInput: string
    searchInputRef: React.RefObject<HTMLInputElement | null>
    setSearchInput: React.Dispatch<React.SetStateAction<string>>
    patchUrl: CustomerReceivablesPatchUrl
    clearFilters: () => void
    onRefresh: () => void
}

export function CustomerReceivablesToolbar({
    view,
    due,
    status,
    reviewStatus,
    counterpartyPartyId,
    customerId,
    lockedCustomerName,
    hasActiveFilters,
    total,
    searchInput,
    searchInputRef,
    setSearchInput,
    patchUrl,
    clearFilters,
    onRefresh,
}: CustomerReceivablesToolbarProps) {
    return (
        <ListToolbar
            search={
                <InputGroup className="max-w-sm">
                    <InputGroupAddon>
                        <SearchIcon aria-hidden="true" />
                    </InputGroupAddon>
                    <InputGroupInput
                        ref={searchInputRef}
                        placeholder="往来主体、销售单、回款单、发票号"
                        value={searchInput}
                        onChange={(e) => setSearchInput(e.target.value)}
                        onKeyDown={(e) => {
                            if (e.key === "Enter") {
                                patchUrl(
                                    {
                                        q: searchInput.trim() || null,
                                        page: null,
                                    },
                                    { replace: true },
                                )
                            }
                        }}
                        aria-label="搜索客户往来"
                    />
                </InputGroup>
            }
            filters={
                <>
                    <label className="flex items-center gap-1.5 text-sm">
                        <span className="sr-only sm:not-sr-only sm:text-muted-foreground">
                            往来主体
                        </span>
                        <ReceivableCounterpartySearchCombobox
                            value={counterpartyPartyId || undefined}
                            onValueChange={(id) => {
                                patchUrl(
                                    {
                                        counterpartyId: id || null,
                                        page: null,
                                    },
                                    { replace: true },
                                )
                            }}
                            purpose="filter"
                            className="w-56"
                            aria-label="筛选往来主体"
                            placeholder="全部主体"
                        />
                    </label>
                    {view === "receivable" ? (
                        <>
                            <label className="flex items-center gap-1.5 text-sm">
                                <span className="sr-only sm:not-sr-only sm:text-muted-foreground">
                                    到期
                                </span>
                                <OptionCombobox
                                    value={due ?? "all"}
                                    onValueChange={(v) => {
                                        const next = v ?? "all"
                                        patchUrl(
                                            {
                                                due:
                                                    next === "all"
                                                        ? null
                                                        : next,
                                                page: null,
                                            },
                                            {
                                                replace: true,
                                            },
                                        )
                                    }}
                                    options={(
                                        Object.keys(DUE_LABEL) as DueFilter[]
                                    ).map((k) => ({
                                        value: k,
                                        label: DUE_LABEL[k],
                                    }))}
                                    className="w-32"
                                    size="sm"
                                    allowClear={false}
                                    aria-label="筛选到期"
                                    placeholder="到期"
                                />
                            </label>
                            <label className="flex items-center gap-1.5 text-sm">
                                <span className="sr-only sm:not-sr-only sm:text-muted-foreground">
                                    状态
                                </span>
                                <OptionCombobox
                                    value={status ?? ""}
                                    onValueChange={(v) => {
                                        patchUrl(
                                            {
                                                status: v || null,
                                                page: null,
                                            },
                                            {
                                                replace: true,
                                            },
                                        )
                                    }}
                                    options={[
                                        {
                                            value: "",
                                            label: "全部状态",
                                        },
                                        {
                                            value: "open",
                                            label: "未结",
                                        },
                                        {
                                            value: "partial",
                                            label: "部分结清",
                                        },
                                        {
                                            value: "settled",
                                            label: "已结清",
                                        },
                                    ]}
                                    className="w-32"
                                    size="sm"
                                    allowClear={false}
                                    aria-label="筛选状态"
                                    placeholder="状态"
                                />
                            </label>
                        </>
                    ) : null}
                </>
            }
            secondary={
                customerId || view === "receivable" ? (
                    <>
                        {customerId ? (
                            <FilterChip
                                label={
                                    lockedCustomerName
                                        ? `经营客户 ${lockedCustomerName}`
                                        : "经营客户锁定"
                                }
                                onClear={() =>
                                    patchUrl(
                                        {
                                            customerId: null,
                                        },
                                        { replace: true },
                                    )
                                }
                                clearLabel="清除客户筛选"
                            />
                        ) : null}
                        {view === "receivable" ? (
                            <label className="flex items-center gap-1.5 text-sm">
                                <span className="sr-only sm:not-sr-only sm:text-muted-foreground">
                                    复核状态
                                </span>
                                <OptionCombobox
                                    value={reviewStatus ?? ""}
                                    onValueChange={(v) => {
                                        patchUrl(
                                            {
                                                reviewStatus: v || null,
                                                page: null,
                                            },
                                            {
                                                replace: true,
                                            },
                                        )
                                    }}
                                    options={[
                                        {
                                            value: "",
                                            label: "全部复核状态",
                                        },
                                        {
                                            value: "pending_opening",
                                            label: "期初待复核",
                                        },
                                        {
                                            value: "reviewed",
                                            label: "已复核",
                                        },
                                        {
                                            value: "pending_sync_diff",
                                            label: "同步差额待复核",
                                        },
                                    ]}
                                    className="w-40"
                                    size="sm"
                                    allowClear={false}
                                    aria-label="筛选复核状态"
                                    placeholder="复核状态"
                                />
                            </label>
                        ) : null}
                    </>
                ) : undefined
            }
            actions={
                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                    <span aria-live="polite">
                        共 {total.toLocaleString("zh-CN")} 条
                    </span>
                    {hasActiveFilters ? (
                        <Button
                            type="button"
                            size="xs"
                            variant="ghost"
                            onClick={clearFilters}
                        >
                            清除筛选
                        </Button>
                    ) : null}
                    <Button
                        type="button"
                        size="sm"
                        variant="ghost"
                        className="text-muted-foreground hover:text-foreground"
                        onClick={onRefresh}
                    >
                        <RefreshCwIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        刷新
                    </Button>
                </div>
            }
        />
    )
}
