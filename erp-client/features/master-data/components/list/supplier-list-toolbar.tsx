"use client"

import * as React from "react"
import { ChevronDownIcon, FilterIcon, SearchIcon } from "lucide-react"

import {
    FixedOptionCheckboxFilter,
    FixedOptionRadioFilter,
    ListToolbar,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { ListSearchField } from "@/features/master-data/components/list/list-search-field"
import { ListToolbarCount } from "@/features/master-data/components/list/list-toolbar-actions"
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
import type { SupplierQualificationHealth } from "@/features/master-data/types"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

export function SupplierListToolbar({
    searchInputRef,
    searchDraft,
    setSearchDraft,
    rowCount,
    hasActiveFilters,
    clearAllFilters,
    supplierFilterPanelOpen,
    setSupplierFilterPanelOpen,
    hasStructuredSupplierFilters,
    applySupplierFilters,
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
    rowCount: number
    hasActiveFilters: boolean
    clearAllFilters: () => void
    supplierFilterPanelOpen: boolean
    setSupplierFilterPanelOpen: SetState<boolean>
    hasStructuredSupplierFilters: boolean
    applySupplierFilters: () => void
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
                    <>
                        {!supplierFilterPanelOpen ? (
                            <Button type="submit" size="sm">
                                <SearchIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                搜索
                            </Button>
                        ) : null}
                        <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            aria-expanded={supplierFilterPanelOpen}
                            onClick={() =>
                                setSupplierFilterPanelOpen((open) => !open)
                            }
                        >
                            <FilterIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            高级筛选
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
                    </>
                }
                secondary={
                    supplierFilterPanelOpen ? (
                        <div
                            className="flex w-full flex-col gap-3 rounded-lg border border-border/60 bg-muted/30 px-3 py-3"
                            aria-label="供应商筛选条件"
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
                            <div className="flex justify-end">
                                <Button type="submit" size="sm">
                                    <SearchIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    搜索
                                </Button>
                            </div>
                        </div>
                    ) : undefined
                }
                actions={
                    <ListToolbarCount
                        label="供应商与资质"
                        rowCount={rowCount}
                        hasActiveFilters={hasActiveFilters}
                        onClear={clearAllFilters}
                    />
                }
            />
        </form>
    )
}
