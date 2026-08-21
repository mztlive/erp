"use client"

import * as React from "react"
import { ChevronDownIcon, FilterIcon, SearchIcon } from "lucide-react"

import {
    FilterChip,
    FixedOptionCheckboxFilter,
    FixedOptionRadioFilter,
    ListToolbar,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { ListSearchField } from "@/features/master-data/components/list/list-search-field"
import {
    masterDataCopy,
    masterDataSearchPlaceholder,
} from "@/features/master-data/lib/copy"
import {
    LIFECYCLE_RADIO_FILTER_OPTIONS,
    SUPPLIER_CAPABILITY_OPTIONS,
    SUPPLIER_QUALIFICATION_HEALTH_OPTIONS,
    SUPPLIER_QUALIFICATION_TYPE_OPTIONS,
} from "@/features/master-data/lib/list-filters"
import type { SupplierAppliedChip } from "@/features/master-data/hooks/use-supplier-list-state"
import type { SupplierFilterKey } from "@/features/master-data/hooks/use-supplier-list-filters"
import type { SupplierQualificationHealth } from "@/features/master-data/types"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

export function SupplierListToolbar({
    searchInputRef,
    searchDraft,
    setSearchDraft,
    hasActiveFilters,
    clearAllFilters,
    appliedChips,
    removeFilter,
    supplierFilterPanelOpen,
    setSupplierFilterPanelOpen,
    hasStructuredSupplierFilters,
    applySupplierFilters,
    resetMoreFilters,
    lifecycleStatusDraft,
    setLifecycleStatusDraft,
    supplierQualificationHealthDraft,
    setSupplierQualificationHealthDraft,
    supplierCapabilityCodesDraft,
    setSupplierCapabilityCodesDraft,
    supplierQualificationTypesDraft,
    setSupplierQualificationTypesDraft,
}: {
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: SetState<string>
    hasActiveFilters: boolean
    clearAllFilters: () => void
    appliedChips: readonly SupplierAppliedChip[]
    removeFilter: (key: SupplierFilterKey) => void
    supplierFilterPanelOpen: boolean
    setSupplierFilterPanelOpen: SetState<boolean>
    hasStructuredSupplierFilters: boolean
    applySupplierFilters: () => void
    resetMoreFilters: () => void
    lifecycleStatusDraft: "enabled" | "disabled" | "all"
    setLifecycleStatusDraft: SetState<"enabled" | "disabled" | "all">
    supplierQualificationHealthDraft: SupplierQualificationHealth | "all"
    setSupplierQualificationHealthDraft: SetState<
        SupplierQualificationHealth | "all"
    >
    supplierCapabilityCodesDraft: string[]
    setSupplierCapabilityCodesDraft: SetState<string[]>
    supplierQualificationTypesDraft: string[]
    setSupplierQualificationTypesDraft: SetState<string[]>
}) {
    const panelId = React.useId()
    const hasChips = hasActiveFilters && appliedChips.length > 0

    return (
        <form
            onSubmit={(event) => {
                event.preventDefault()
                applySupplierFilters()
            }}
        >
            <ListToolbar
                search={
                    <ListSearchField
                        searchInputRef={searchInputRef}
                        value={searchDraft}
                        onChange={setSearchDraft}
                        placeholder={masterDataSearchPlaceholder("suppliers")}
                    />
                }
                filters={
                    <Button
                        type="button"
                        variant="outline"
                        aria-expanded={supplierFilterPanelOpen}
                        aria-controls={panelId}
                        onClick={() =>
                            setSupplierFilterPanelOpen((open) => !open)
                        }
                    >
                        <FilterIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        更多筛选
                        {hasStructuredSupplierFilters ? (
                            <Badge variant="info">已启用</Badge>
                        ) : null}
                        <ChevronDownIcon
                            data-icon="inline-end"
                            aria-hidden="true"
                            className={
                                supplierFilterPanelOpen
                                    ? "rotate-180 transition-transform"
                                    : "transition-transform"
                            }
                        />
                    </Button>
                }
                secondary={
                    hasChips || supplierFilterPanelOpen ? (
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
                            {supplierFilterPanelOpen ? (
                                <div
                                    id={panelId}
                                    className="flex w-full flex-col gap-3 border-t pt-3"
                                    aria-label="供应商与资质更多筛选条件"
                                >
                                    <FixedOptionRadioFilter
                                        label="启停"
                                        value={lifecycleStatusDraft}
                                        onValueChange={setLifecycleStatusDraft}
                                        options={LIFECYCLE_RADIO_FILTER_OPTIONS}
                                        aria-label={masterDataCopy.filterLifecycleAria}
                                    />
                                    <FixedOptionRadioFilter
                                        label="资质状态"
                                        value={supplierQualificationHealthDraft}
                                        onValueChange={
                                            setSupplierQualificationHealthDraft
                                        }
                                        options={SUPPLIER_QUALIFICATION_HEALTH_OPTIONS}
                                        aria-label="资质状态"
                                    />
                                    <FixedOptionCheckboxFilter
                                        label="供应能力"
                                        value={supplierCapabilityCodesDraft}
                                        onValueChange={setSupplierCapabilityCodesDraft}
                                        options={SUPPLIER_CAPABILITY_OPTIONS}
                                        aria-label="供应能力，可多选"
                                    />
                                    <FixedOptionCheckboxFilter
                                        label="资质类型"
                                        value={supplierQualificationTypesDraft}
                                        onValueChange={
                                            setSupplierQualificationTypesDraft
                                        }
                                        options={SUPPLIER_QUALIFICATION_TYPE_OPTIONS}
                                        aria-label="资质类型，可多选"
                                    />
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
