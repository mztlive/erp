"use client"

import { SearchIcon } from "lucide-react"

import { ListToolbar, OptionCombobox } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import type { PurchaseOrderStatusFilter } from "@/features/purchase-orders/types"
import { PO_STATUS_FILTER_LABEL } from "@/features/purchase-orders/types"

/** 状态枚举 ≥5：用 Combobox，禁止长 Toggle 横排（ui-filter-design §3.2） */
const PO_STATUS_FILTER_OPTIONS = (
    Object.entries(PO_STATUS_FILTER_LABEL) as Array<
        [PurchaseOrderStatusFilter, string]
    >
)
    .filter(([value]) => value !== "all")
    .map(([value, label]) => ({ value, label }))

export type PurchaseOrdersListToolbarProps = {
    searchDraft: string
    onSearchDraftChange: (value: string) => void
    statusFilter: PurchaseOrderStatusFilter
    onStatusFilterChange: (value: PurchaseOrderStatusFilter) => void
    total: number
    hasActiveFilters: boolean
    onClearFilters: () => void
}

export function PurchaseOrdersListToolbar({
    searchDraft,
    onSearchDraftChange,
    statusFilter,
    onStatusFilterChange,
    total,
    hasActiveFilters,
    onClearFilters,
}: PurchaseOrdersListToolbarProps) {
    return (
        <ListToolbar
            search={
                <InputGroup>
                    <InputGroupAddon>
                        <SearchIcon aria-hidden="true" />
                    </InputGroupAddon>
                    <InputGroupInput
                        data-slot="po-list-search"
                        value={searchDraft}
                        onChange={(event) => {
                            onSearchDraftChange(event.target.value)
                        }}
                        placeholder="采购单号、供应商、来源销售单"
                        aria-label="搜索采购单"
                    />
                </InputGroup>
            }
            filters={
                <OptionCombobox
                    value={statusFilter === "all" ? null : statusFilter}
                    options={PO_STATUS_FILTER_OPTIONS}
                    placeholder="状态：全部"
                    size="sm"
                    aria-label="按状态筛选"
                    inputClassName="w-[9.5rem]"
                    onValueChange={(v) => {
                        onStatusFilterChange(
                            (v as PurchaseOrderStatusFilter | null) ?? "all",
                        )
                    }}
                />
            }
            actions={
                <div className="flex items-center gap-2">
                    <span
                        className="text-xs text-muted-foreground"
                        aria-live="polite"
                    >
                        共 {total.toLocaleString("zh-CN")} 条
                    </span>
                    {hasActiveFilters ? (
                        <Button
                            type="button"
                            size="xs"
                            variant="ghost"
                            onClick={onClearFilters}
                        >
                            清除筛选
                        </Button>
                    ) : null}
                </div>
            }
        />
    )
}
