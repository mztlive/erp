"use client"

import * as React from "react"
import { SearchIcon } from "lucide-react"

import { ListToolbar, OptionCombobox } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { WarehouseSearchCombobox } from "@/features/entity-selectors"
import { ChipFilter } from "@/features/inventory/components/presentation"
import {
    defaultSortValue,
    MOVEMENT_TYPE_OPTIONS,
    sortOptions,
} from "@/features/inventory/lib/presentation"
import { AVAILABILITY_LABEL } from "@/features/inventory/types"
import type {
    InventoryAvailability,
    InventoryView,
} from "@/features/inventory/types"

interface LedgerToolbarProps {
    view: InventoryView
    searchInput: string
    searchInputRef: React.RefObject<HTMLInputElement | null>
    onSearchChange: (value: string) => void
    warehouseId: string | undefined
    availability: InventoryAvailability
    movementType: string[]
    occurredFrom: string | undefined
    occurredTo: string | undefined
    sortValue: string
    total: number
    hasActiveFilters: boolean
    skuId: string | undefined
    salesOrderLineId: string | undefined
    adjustmentIdParam: string | undefined
    chipSkuName: string | undefined
    chipSalesLineLabel: string | undefined
    chipAdjustmentNo: string | undefined
    onApplyPatch: (
        patch: Record<string, string | null | undefined>,
    ) => void
    onClearAll: () => void
}

export function LedgerToolbar({
    view,
    searchInput,
    searchInputRef,
    onSearchChange,
    warehouseId,
    availability,
    movementType,
    occurredFrom,
    occurredTo,
    sortValue,
    total,
    hasActiveFilters,
    skuId,
    salesOrderLineId,
    adjustmentIdParam,
    chipSkuName,
    chipSalesLineLabel,
    chipAdjustmentNo,
    onApplyPatch,
    onClearAll,
}: LedgerToolbarProps) {
    return (
        <ListToolbar
            search={
                <InputGroup>
                    <InputGroupAddon>
                        <SearchIcon aria-hidden="true" />
                    </InputGroupAddon>
                    <InputGroupInput
                        ref={searchInputRef}
                        value={searchInput}
                        onChange={(e) => {
                            onSearchChange(e.target.value)
                        }}
                        placeholder="SKU 编码、名称、规格、仓库"
                        aria-label="搜索库存"
                    />
                </InputGroup>
            }
            filters={
                <>
                    <label className="flex items-center gap-1.5 text-sm">
                        <span className="sr-only sm:not-sr-only sm:text-muted-foreground">
                            仓库
                        </span>
                        <WarehouseSearchCombobox
                            className="w-44"
                            value={warehouseId || undefined}
                            onValueChange={(id) => {
                                onApplyPatch({
                                    warehouseId: id || null,
                                })
                            }}
                            purpose="filter"
                            aria-label="筛选仓库"
                            placeholder="全部仓库"
                        />
                    </label>
                    {view === "balance" ? (
                        <label className="flex items-center gap-1.5 text-sm">
                            <span className="sr-only sm:not-sr-only sm:text-muted-foreground">
                                可用状态
                            </span>
                            <OptionCombobox
                                className="w-28"
                                value={availability}
                                onValueChange={(v) => {
                                    onApplyPatch({
                                        availability: (v ??
                                            "all") as InventoryAvailability,
                                    })
                                }}
                                options={(
                                    [
                                        "all",
                                        "positive",
                                        "zero",
                                        "reserved",
                                    ] as const
                                ).map((a) => ({
                                    value: a,
                                    label: AVAILABILITY_LABEL[a],
                                }))}
                                size="sm"
                                allowClear={false}
                                aria-label="筛选可用状态"
                                placeholder="可用状态"
                            />
                        </label>
                    ) : null}
                    {view === "movement" ? (
                        <>
                            <label className="flex items-center gap-1.5 text-sm">
                                <span className="sr-only">流水类型</span>
                                <OptionCombobox
                                    className="w-32"
                                    value={movementType[0] ?? "all"}
                                    onValueChange={(value) => {
                                        onApplyPatch({
                                            movementType:
                                                value && value !== "all"
                                                    ? value
                                                    : null,
                                        })
                                    }}
                                    options={[
                                        {
                                            value: "all",
                                            label: "全部流水",
                                        },
                                        ...MOVEMENT_TYPE_OPTIONS,
                                    ]}
                                    size="sm"
                                    allowClear={false}
                                    aria-label="筛选流水类型"
                                    placeholder="全部流水"
                                />
                            </label>
                            <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
                                发生日期
                                <Input
                                    type="date"
                                    className="h-8 w-32"
                                    value={occurredFrom ?? ""}
                                    max={occurredTo}
                                    onChange={(event) => {
                                        onApplyPatch({
                                            occurredFrom:
                                                event.target.value,
                                        })
                                    }}
                                    aria-label="发生日期起"
                                />
                                <span>至</span>
                                <Input
                                    type="date"
                                    className="h-8 w-32"
                                    value={occurredTo ?? ""}
                                    min={occurredFrom}
                                    onChange={(event) => {
                                        onApplyPatch({
                                            occurredTo: event.target.value,
                                        })
                                    }}
                                    aria-label="发生日期止"
                                />
                            </label>
                        </>
                    ) : null}
                </>
            }
            secondary={
                skuId || salesOrderLineId || adjustmentIdParam ? (
                    <>
                        {skuId ? (
                            <ChipFilter
                                label={`当前 SKU：${chipSkuName ?? "已定位单品"}`}
                                onClear={() => {
                                    onApplyPatch({ skuId: null })
                                }}
                            />
                        ) : null}
                        {salesOrderLineId ? (
                            <ChipFilter
                                label={`销售单明细：${chipSalesLineLabel ?? "已定位"}`}
                                onClear={() => {
                                    onApplyPatch({ salesOrderLineId: null })
                                }}
                            />
                        ) : null}
                        {adjustmentIdParam ? (
                            <ChipFilter
                                label={`调整单：${chipAdjustmentNo ?? "已定位"}`}
                                onClear={() => {
                                    onApplyPatch({ adjustmentId: null })
                                }}
                            />
                        ) : null}
                    </>
                ) : undefined
            }
            actions={
                <>
                    <label className="flex items-center gap-1.5 text-sm">
                        <span className="sr-only">排序</span>
                        <OptionCombobox
                            className="w-40"
                            value={sortValue}
                            onValueChange={(value) => {
                                onApplyPatch({
                                    sort:
                                        value ??
                                        defaultSortValue(view),
                                })
                            }}
                            options={sortOptions(view)}
                            size="sm"
                            allowClear={false}
                            aria-label="排序方式"
                            placeholder="排序"
                        />
                    </label>
                    <span
                        className="text-xs text-muted-foreground"
                        aria-live="polite"
                    >
                        共 {total.toLocaleString("zh-CN")} 条
                    </span>
                    {hasActiveFilters && (
                        <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={onClearAll}
                        >
                            清除筛选
                        </Button>
                    )}
                </>
            }
        />
    )
}
