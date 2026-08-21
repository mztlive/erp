"use client"

import * as React from "react"
import {
    ChevronDownIcon,
    FilterIcon,
    SearchIcon,
} from "lucide-react"

import {
    FilterChip,
    FixedOptionRadioFilter,
    ListToolbar,
    MultiOptionCombobox,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { WarehouseSearchCombobox } from "@/features/entity-selectors"
import { MOVEMENT_TYPE_OPTIONS } from "@/features/inventory/lib/presentation"
import type {
    LedgerAppliedChip,
    LedgerFilterKey,
} from "@/features/inventory/pages/hooks/use-ledger-filters"
import { AVAILABILITY_LABEL } from "@/features/inventory/types"
import type {
    InventoryAvailability,
    InventoryView,
} from "@/features/inventory/types"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

const AVAILABILITY_RADIO_OPTIONS: ReadonlyArray<{
    value: InventoryAvailability
    label: string
}> = (["all", "positive", "zero", "reserved"] as const).map((value) => ({
    value,
    label: AVAILABILITY_LABEL[value],
}))

interface LedgerToolbarProps {
    view: InventoryView
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: SetState<string>
    warehouseIdDraft: string | null
    setWarehouseIdDraft: SetState<string | null>
    availabilityDraft: InventoryAvailability
    setAvailabilityDraft: SetState<InventoryAvailability>
    movementTypeDraft: string[]
    setMovementTypeDraft: SetState<string[]>
    occurredFromDraft: string
    setOccurredFromDraft: SetState<string>
    occurredToDraft: string
    setOccurredToDraft: SetState<string>
    panelOpen: boolean
    setPanelOpen: SetState<boolean>
    hasStructuredFilters: boolean
    hasActiveFilters: boolean
    appliedChips: readonly LedgerAppliedChip[]
    removeFilter: (key: LedgerFilterKey) => void
    applyFilters: () => void
    resetMoreFilters: () => void
    clearAllFilters: () => void
    filterError: string | null
    setFilterError: SetState<string | null>
}

/**
 * 库存台账筛选条（docs/ui-filter-design.md §3 / §8.2）：
 * 单一 form；关键词 +「更多筛选」；已生效条件以 chip 展示；
 * 收起态 Enter 与展开态「应用全部筛选」共用 applyFilters。
 */
export function LedgerToolbar({
    view,
    searchInputRef,
    searchDraft,
    setSearchDraft,
    warehouseIdDraft,
    setWarehouseIdDraft,
    availabilityDraft,
    setAvailabilityDraft,
    movementTypeDraft,
    setMovementTypeDraft,
    occurredFromDraft,
    setOccurredFromDraft,
    occurredToDraft,
    setOccurredToDraft,
    panelOpen,
    setPanelOpen,
    hasStructuredFilters,
    hasActiveFilters,
    appliedChips,
    removeFilter,
    applyFilters,
    resetMoreFilters,
    clearAllFilters,
    filterError,
    setFilterError,
}: LedgerToolbarProps) {
    const panelId = React.useId()
    const dateErrorId = React.useId()
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
                            value={searchDraft}
                            onChange={(event) =>
                                setSearchDraft(event.target.value)
                            }
                            placeholder="SKU 编码、名称、规格、仓库"
                            aria-label="搜索库存"
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
                                    aria-label="库存台账更多筛选条件"
                                >
                                    {view === "balance" ? (
                                        <FixedOptionRadioFilter
                                            label="可用状态"
                                            value={availabilityDraft}
                                            onValueChange={
                                                setAvailabilityDraft
                                            }
                                            options={
                                                AVAILABILITY_RADIO_OPTIONS
                                            }
                                        />
                                    ) : null}
                                    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                仓库
                                            </span>
                                            <WarehouseSearchCombobox
                                                className="w-full"
                                                value={
                                                    warehouseIdDraft ??
                                                    undefined
                                                }
                                                onValueChange={(id) =>
                                                    setWarehouseIdDraft(
                                                        id ?? null,
                                                    )
                                                }
                                                purpose="filter"
                                                aria-label="筛选仓库"
                                                placeholder="全部仓库"
                                            />
                                        </div>
                                        {view === "movement" ? (
                                            <>
                                                <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                                    <span className="text-muted-foreground">
                                                        流水类型
                                                    </span>
                                                    <MultiOptionCombobox
                                                        className="w-full"
                                                        value={
                                                            movementTypeDraft
                                                        }
                                                        onValueChange={
                                                            setMovementTypeDraft
                                                        }
                                                        options={
                                                            MOVEMENT_TYPE_OPTIONS
                                                        }
                                                        placeholder="全部流水类型"
                                                        aria-label="流水类型"
                                                    />
                                                </div>
                                                <div className="flex min-w-0 flex-col gap-1.5 text-sm sm:col-span-2">
                                                    <span className="text-muted-foreground">
                                                        发生日期
                                                    </span>
                                                    <div className="flex items-center gap-1.5">
                                                        <Input
                                                            type="date"
                                                            className="w-0 min-w-0 flex-1"
                                                            value={
                                                                occurredFromDraft
                                                            }
                                                            max={
                                                                occurredToDraft ||
                                                                undefined
                                                            }
                                                            onChange={(event) => {
                                                                setOccurredFromDraft(
                                                                    event.target
                                                                        .value,
                                                                )
                                                                setFilterError(
                                                                    null,
                                                                )
                                                            }}
                                                            autoComplete="off"
                                                            aria-label="发生日期起"
                                                            aria-invalid={Boolean(
                                                                filterError,
                                                            )}
                                                            aria-describedby={
                                                                filterError
                                                                    ? dateErrorId
                                                                    : undefined
                                                            }
                                                        />
                                                        <span className="text-muted-foreground">
                                                            至
                                                        </span>
                                                        <Input
                                                            type="date"
                                                            className="w-0 min-w-0 flex-1"
                                                            value={
                                                                occurredToDraft
                                                            }
                                                            min={
                                                                occurredFromDraft ||
                                                                undefined
                                                            }
                                                            onChange={(event) => {
                                                                setOccurredToDraft(
                                                                    event.target
                                                                        .value,
                                                                )
                                                                setFilterError(
                                                                    null,
                                                                )
                                                            }}
                                                            autoComplete="off"
                                                            aria-label="发生日期止"
                                                            aria-invalid={Boolean(
                                                                filterError,
                                                            )}
                                                            aria-describedby={
                                                                filterError
                                                                    ? dateErrorId
                                                                    : undefined
                                                            }
                                                        />
                                                    </div>
                                                </div>
                                            </>
                                        ) : null}
                                    </div>
                                    {filterError ? (
                                        <span
                                            id={dateErrorId}
                                            className="text-xs text-destructive"
                                            role="alert"
                                        >
                                            {filterError}
                                        </span>
                                    ) : null}
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
