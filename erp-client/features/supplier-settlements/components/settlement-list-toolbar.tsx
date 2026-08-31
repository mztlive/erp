"use client"

import * as React from "react"
import { ChevronDownIcon, FilterIcon, SearchIcon } from "lucide-react"

import {
    FilterChip,
    FixedOptionRadioFilter,
    ListToolbar,
    MultiOptionCombobox,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { DatePicker } from "@/components/ui/date-picker"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { SupplierSearchCombobox } from "@/features/entity-selectors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { SettlementsUrlState } from "@/features/supplier-settlements/lib/url-state"
import {
    buildSettlementFilterChips,
    DIFF_TYPE_RADIO_OPTIONS,
    hasStructuredSettlementFilters,
    SETTLEMENT_STATUS_VALUES,
    type SettlementFilterKey,
} from "@/features/supplier-settlements/lib/settlement-list-filters"
import {
    STATUS_LABEL,
    type DifferenceType,
} from "@/features/supplier-settlements/types"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

const STATUS_FILTER_OPTIONS = SETTLEMENT_STATUS_VALUES.map((value) => ({
    value,
    label: STATUS_LABEL[value],
}))

export function SettlementListToolbar({
    urlState,
    suppliers,
    searchInputRef,
    searchDraft,
    setSearchDraft,
    panelOpen,
    setPanelOpen,
    hasActiveFilters,
    applyFilters,
    removeFilter,
    resetMoreFilters,
    clearAllFilters,
    supplierIdDraft,
    setSupplierIdDraft,
    statusDraft,
    setStatusDraft,
    differenceTypeDraft,
    setDifferenceTypeDraft,
    periodFromDraft,
    setPeriodFromDraft,
    periodToDraft,
    setPeriodToDraft,
    periodError,
    setPeriodError,
}: {
    urlState: SettlementsUrlState
    suppliers: readonly { supplierId: string; supplierName: string }[]
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: SetState<string>
    panelOpen: boolean
    setPanelOpen: SetState<boolean>
    hasActiveFilters: boolean
    applyFilters: () => void
    removeFilter: (key: SettlementFilterKey) => void
    resetMoreFilters: () => void
    clearAllFilters: () => void
    supplierIdDraft: string | null
    setSupplierIdDraft: SetState<string | null>
    statusDraft: string[]
    setStatusDraft: SetState<string[]>
    differenceTypeDraft: DifferenceType | "all"
    setDifferenceTypeDraft: SetState<DifferenceType | "all">
    periodFromDraft: string
    setPeriodFromDraft: SetState<string>
    periodToDraft: string
    setPeriodToDraft: SetState<string>
    periodError: string | null
    setPeriodError: SetState<string | null>
}) {
    const panelId = React.useId()
    const periodErrorId = React.useId()
    const appliedChips = React.useMemo(
        () => buildSettlementFilterChips(urlState, suppliers),
        [suppliers, urlState],
    )
    const hasChips = hasActiveFilters && appliedChips.length > 0
    const hasStructuredFilters = hasStructuredSettlementFilters(urlState)

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
                            id="supplier-settlements-list-search-input"
                            ref={searchInputRef}
                            value={searchDraft}
                            onChange={(event) =>
                                setSearchDraft(event.target.value)
                            }
                            placeholder="结算单号、外部账单号、供应商"
                            aria-label="搜索结算单"
                            data-slot="settlement-list-search"
                        />
                    </InputGroup>
                }
                filters={
                    <Button
                        id="supplier-settlements-list-filter-toggle"
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
                                            id={`supplier-settlements-list-filter-chip-${toAutomationIdSegment(chip.key)}`}
                                            label={chip.label}
                                            clearLabel={`移除${chip.label}`}
                                            onClear={() =>
                                                removeFilter(chip.key)
                                            }
                                        />
                                    ))}
                                    <Button
                                        id="supplier-settlements-list-filter-clear-all"
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
                                    aria-label="结算单列表更多筛选条件"
                                >
                                    <FixedOptionRadioFilter
                                        id="supplier-settlements-list-filter-difference-type"
                                        label="差异类型"
                                        value={differenceTypeDraft}
                                        onValueChange={setDifferenceTypeDraft}
                                        options={DIFF_TYPE_RADIO_OPTIONS}
                                    />
                                    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                供应商
                                            </span>
                                            <SupplierSearchCombobox
                                                id="supplier-settlements-list-filter-supplier"
                                                purpose="filter"
                                                className="w-full"
                                                value={
                                                    supplierIdDraft ?? undefined
                                                }
                                                onValueChange={(id) =>
                                                    setSupplierIdDraft(
                                                        id ?? null,
                                                    )
                                                }
                                                aria-label="供应商"
                                                placeholder="全部供应商"
                                            />
                                        </div>
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                状态
                                            </span>
                                            <MultiOptionCombobox
                                                id="supplier-settlements-list-filter-status"
                                                className="w-full"
                                                value={statusDraft}
                                                onValueChange={setStatusDraft}
                                                options={STATUS_FILTER_OPTIONS}
                                                placeholder="全部状态"
                                                aria-label="状态"
                                            />
                                        </div>
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm sm:col-span-2">
                                            <span className="text-muted-foreground">
                                                结算期间
                                            </span>
                                            <div
                                                className="flex items-center gap-1.5"
                                                role="group"
                                                aria-label="结算期间"
                                                aria-describedby={
                                                    periodError
                                                        ? periodErrorId
                                                        : undefined
                                                }
                                            >
                                                <DatePicker
                                                    id="supplier-settlements-list-filter-period-from"
                                                    className="w-0 min-w-0 flex-1"
                                                    value={
                                                        periodFromDraft ||
                                                        undefined
                                                    }
                                                    onValueChange={(next) => {
                                                        setPeriodFromDraft(
                                                            next ?? "",
                                                        )
                                                        setPeriodError(null)
                                                    }}
                                                    aria-invalid={Boolean(
                                                        periodError,
                                                    )}
                                                    placeholder="期间自"
                                                />
                                                <span className="text-muted-foreground">
                                                    至
                                                </span>
                                                <DatePicker
                                                    id="supplier-settlements-list-filter-period-to"
                                                    className="w-0 min-w-0 flex-1"
                                                    value={
                                                        periodToDraft ||
                                                        undefined
                                                    }
                                                    onValueChange={(next) => {
                                                        setPeriodToDraft(
                                                            next ?? "",
                                                        )
                                                        setPeriodError(null)
                                                    }}
                                                    aria-invalid={Boolean(
                                                        periodError,
                                                    )}
                                                    placeholder="期间至"
                                                />
                                            </div>
                                            {periodError ? (
                                                <span
                                                    id={periodErrorId}
                                                    className="text-xs text-destructive"
                                                    role="alert"
                                                >
                                                    {periodError}
                                                </span>
                                            ) : null}
                                        </div>
                                    </div>
                                    <div className="flex flex-col gap-3 border-t pt-3 sm:flex-row sm:items-center sm:justify-between">
                                        <p className="text-xs text-muted-foreground">
                                            将同时应用上方关键词和以下筛选条件；结果也用于导出。
                                        </p>
                                        <div className="flex flex-wrap items-center gap-2 sm:justify-end">
                                            <Button
                                                id="supplier-settlements-list-filter-reset-more"
                                                type="button"
                                                variant="ghost"
                                                onClick={resetMoreFilters}
                                            >
                                                重置更多条件
                                            </Button>
                                            <Button
                                                id="supplier-settlements-list-filter-apply"
                                                type="submit"
                                            >
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
