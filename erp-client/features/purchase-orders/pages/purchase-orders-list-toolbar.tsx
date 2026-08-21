"use client"

import * as React from "react"
import { ChevronDownIcon, FilterIcon, SearchIcon } from "lucide-react"

import { FilterChip, ListToolbar, OptionCombobox } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import type {
    PurchaseOrderAppliedChip,
    PurchaseOrderFilterKey,
} from "@/features/purchase-orders/hooks/use-purchase-orders-list-filters"
import type { PurchaseOrderStatusFilter } from "@/features/purchase-orders/types"
import { PO_STATUS_FILTER_LABEL } from "@/features/purchase-orders/types"

/** 状态枚举 ≥5：面板内用 Combobox，禁止长 Toggle 横排。 */
const PO_STATUS_FILTER_OPTIONS = (
    Object.entries(PO_STATUS_FILTER_LABEL) as Array<
        [PurchaseOrderStatusFilter, string]
    >
)
    .filter(([value]) => value !== "all")
    .map(([value, label]) => ({ value, label }))

export type PurchaseOrdersListToolbarProps = {
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: React.Dispatch<React.SetStateAction<string>>
    statusDraft: PurchaseOrderStatusFilter
    setStatusDraft: React.Dispatch<
        React.SetStateAction<PurchaseOrderStatusFilter>
    >
    panelOpen: boolean
    setPanelOpen: React.Dispatch<React.SetStateAction<boolean>>
    hasActiveFilters: boolean
    hasStructuredFilters: boolean
    appliedChips: readonly PurchaseOrderAppliedChip[]
    removeFilter: (key: PurchaseOrderFilterKey) => void
    applyFilters: () => void
    resetMoreFilters: () => void
    clearAllFilters: () => void
}

export function PurchaseOrdersListToolbar({
    searchInputRef,
    searchDraft,
    setSearchDraft,
    statusDraft,
    setStatusDraft,
    panelOpen,
    setPanelOpen,
    hasActiveFilters,
    hasStructuredFilters,
    appliedChips,
    removeFilter,
    applyFilters,
    resetMoreFilters,
    clearAllFilters,
}: PurchaseOrdersListToolbarProps) {
    const panelId = React.useId()
    const hasChips = hasActiveFilters && appliedChips.length > 0

    return (
        <form
            onSubmit={(event) => {
                event.preventDefault()
                applyFilters()
            }}
        >
            <ListToolbar
                search={
                    <InputGroup>
                        <InputGroupAddon>
                            <SearchIcon aria-hidden="true" />
                        </InputGroupAddon>
                        <InputGroupInput
                            ref={searchInputRef}
                            data-slot="po-list-search"
                            value={searchDraft}
                            onChange={(event) =>
                                setSearchDraft(event.target.value)
                            }
                            placeholder="采购单号、供应商、来源销售单"
                            aria-label="搜索采购单"
                        />
                    </InputGroup>
                }
                filters={
                    <Button
                        type="button"
                        variant="outline"
                        aria-expanded={panelOpen}
                        aria-controls={panelId}
                        onClick={() => setPanelOpen((open) => !open)}
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
                                panelOpen
                                    ? "rotate-180 transition-transform"
                                    : "transition-transform"
                            }
                        />
                    </Button>
                }
                secondary={
                    hasChips || panelOpen ? (
                        <div className="w-full space-y-3">
                            {hasChips ? (
                                <div className="flex flex-wrap items-center gap-2 border-t pt-3">
                                    <span className="text-xs text-muted-foreground">
                                        已筛选
                                    </span>
                                    {appliedChips.map((chip) => (
                                        <FilterChip
                                            key={chip.key}
                                            label={chip.label}
                                            clearLabel={`移除${chip.label}`}
                                            onClear={() =>
                                                removeFilter(chip.key)
                                            }
                                        />
                                    ))}
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        onClick={clearAllFilters}
                                    >
                                        清空全部
                                    </Button>
                                </div>
                            ) : null}
                            {panelOpen ? (
                                <div
                                    id={panelId}
                                    className="flex w-full flex-col gap-3 border-t pt-3"
                                    aria-label="采购单更多筛选条件"
                                >
                                    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                主状态
                                            </span>
                                            <OptionCombobox
                                                className="w-full"
                                                value={
                                                    statusDraft === "all"
                                                        ? null
                                                        : statusDraft
                                                }
                                                onValueChange={(value) =>
                                                    setStatusDraft(
                                                        (value as PurchaseOrderStatusFilter | null) ??
                                                            "all",
                                                    )
                                                }
                                                options={
                                                    PO_STATUS_FILTER_OPTIONS
                                                }
                                                aria-label="主状态"
                                                placeholder="状态：全部"
                                            />
                                        </div>
                                    </div>
                                    <div className="flex flex-col gap-3 border-t pt-3 sm:flex-row sm:items-center sm:justify-between">
                                        <p className="text-xs text-muted-foreground">
                                            将同时应用上方关键词和以下筛选条件；结果也用于导出。
                                        </p>
                                        <div className="flex flex-wrap items-center gap-2 sm:justify-end">
                                            <Button
                                                type="button"
                                                variant="ghost"
                                                onClick={resetMoreFilters}
                                            >
                                                重置更多条件
                                            </Button>
                                            <Button type="submit">
                                                <SearchIcon
                                                    data-icon="inline-start"
                                                    aria-hidden="true"
                                                />
                                                应用全部筛选
                                            </Button>
                                        </div>
                                    </div>
                                </div>
                            ) : null}
                        </div>
                    ) : undefined
                }
            />
        </form>
    )
}
