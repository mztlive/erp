"use client"

import * as React from "react"
import { ChevronDownIcon, FilterIcon, SearchIcon } from "lucide-react"

import { FilterChip, ListToolbar } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { SalesOrdersAppliedChip } from "@/features/sales-orders/hooks/use-sales-orders-list-chips"
import type { SalesOrderSummaryFilter } from "@/features/sales-orders/lib/filter-orders"
import type { SalesOrdersUrlState } from "@/features/sales-orders/lib/url-state"

const SUMMARY_FILTER_OPTIONS: ReadonlyArray<{
    value: SalesOrderSummaryFilter
    label: string
}> = [
    { value: "all", label: "全部" },
    { value: "mine", label: "待我处理" },
    { value: "createdByMe", label: "我创建的" },
    { value: "exception", label: "异常" },
]

export function SalesOrdersListFilterBar(props: {
    panelId: string
    searchDraft: string
    onSearchDraftChange: (value: string) => void
    onSubmit: () => void
    filterPanelOpen: boolean
    onToggleFilterPanel: () => void
    hasStructuredFilters: boolean
    hasChips: boolean
    chips: readonly SalesOrdersAppliedChip[]
    onClearFilters: () => void
    summary: SalesOrdersUrlState["summary"]
    onSummaryChange: (summary: SalesOrderSummaryFilter) => void
    filterPanel: React.ReactNode
}) {
    const {
        panelId,
        searchDraft,
        onSearchDraftChange,
        onSubmit,
        filterPanelOpen,
        onToggleFilterPanel,
        hasStructuredFilters,
        hasChips,
        chips,
        onClearFilters,
        summary,
        onSummaryChange,
        filterPanel,
    } = props

    return (
        <form
            onSubmit={(event) => {
                event.preventDefault()
                onSubmit()
            }}
        >
            <ListToolbar
                search={
                    <InputGroup>
                        <InputGroupAddon>
                            <SearchIcon aria-hidden="true" />
                        </InputGroupAddon>
                        <InputGroupInput
                            id="sales-orders-list-filter-search"
                            data-slot="so-list-search"
                            value={searchDraft}
                            onChange={(event) => {
                                onSearchDraftChange(event.target.value)
                            }}
                            placeholder="销售单号、客户、合同号"
                            aria-label="搜索销售单"
                        />
                    </InputGroup>
                }
                filters={
                    <>
                        <div
                            role="group"
                            aria-label="销售单工作视图"
                            className="flex h-control max-w-full items-stretch overflow-x-auto rounded-lg border bg-muted/40 p-0.5 [&_[data-slot=button]]:h-full [&_[data-slot=button]]:min-h-0"
                        >
                            {SUMMARY_FILTER_OPTIONS.map((option) => {
                                const active = summary === option.value
                                return (
                                    <Button
                                        key={option.value}
                                        id={`sales-orders-list-filter-summary-${toAutomationIdSegment(option.value)}`}
                                        type="button"
                                        variant={active ? "secondary" : "ghost"}
                                        className={
                                            active
                                                ? "bg-card shadow-xs"
                                                : "shadow-none"
                                        }
                                        aria-pressed={active}
                                        onClick={() =>
                                            onSummaryChange(option.value)
                                        }
                                    >
                                        {option.label}
                                    </Button>
                                )
                            })}
                        </div>
                        <Button
                            id="sales-orders-list-filter-more-toggle"
                            type="button"
                            variant="outline"
                            aria-expanded={filterPanelOpen}
                            aria-controls={panelId}
                            onClick={onToggleFilterPanel}
                        >
                            <FilterIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            更多筛选
                            {hasStructuredFilters ? (
                                <Badge variant="info">已启用</Badge>
                            ) : null}
                            <ChevronDownIcon
                                data-icon="inline-end"
                                aria-hidden="true"
                                className={
                                    filterPanelOpen
                                        ? "rotate-180 transition-transform"
                                        : "transition-transform"
                                }
                            />
                        </Button>
                    </>
                }
                secondary={
                    hasChips || filterPanelOpen ? (
                        <div className="w-full space-y-3">
                            {hasChips ? (
                                <div className="flex flex-wrap items-center gap-2 border-t pt-3">
                                    <span className="text-xs text-muted-foreground">
                                        已筛选
                                    </span>
                                    {chips.map((chip) => (
                                        <FilterChip
                                            key={chip.key}
                                            id={`sales-orders-list-filter-chip-${toAutomationIdSegment(chip.key)}`}
                                            label={chip.label}
                                            clearLabel={chip.clearLabel}
                                            onClear={chip.onClear}
                                        />
                                    ))}
                                    <Button
                                        id="sales-orders-list-filter-clear-all"
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        onClick={onClearFilters}
                                    >
                                        清空全部
                                    </Button>
                                </div>
                            ) : null}
                            {filterPanelOpen ? filterPanel : null}
                        </div>
                    ) : undefined
                }
            />
        </form>
    )
}
