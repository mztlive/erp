"use client"

import * as React from "react"
import { ChevronDownIcon, FilterIcon, SearchIcon } from "lucide-react"

import {
    FilterChip,
    FixedOptionRadioFilter,
    ListToolbar,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { ListSearchField } from "@/features/master-data/components/list/list-search-field"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import {
    LIFECYCLE_RADIO_FILTER_OPTIONS,
    REVISION_TIMING_RADIO_FILTER_OPTIONS,
} from "@/features/master-data/lib/list-filters"
import type {
    DictionaryAppliedChip,
    DictionaryFilterKey,
} from "@/features/master-data/hooks/use-lifecycle-list-filters"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

export function DictionaryListToolbar({
    idPrefix,
    searchInputRef,
    searchDraft,
    setSearchDraft,
    searchPlaceholder,
    countLabel,
    hasActiveFilters,
    clearAllFilters,
    appliedChips,
    removeFilter,
    filterPanelOpen,
    setFilterPanelOpen,
    hasStructuredListFilters,
    applyListFilters,
    resetMoreFilters,
    lifecycleStatusDraft,
    setLifecycleStatusDraft,
    revisionTimingDraft,
    setRevisionTimingDraft,
}: {
    idPrefix?: string
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: SetState<string>
    searchPlaceholder: string
    countLabel: string
    hasActiveFilters: boolean
    clearAllFilters: () => void
    appliedChips: readonly DictionaryAppliedChip[]
    removeFilter: (key: DictionaryFilterKey) => void
    filterPanelOpen: boolean
    setFilterPanelOpen: SetState<boolean>
    hasStructuredListFilters: boolean
    applyListFilters: () => void
    resetMoreFilters: () => void
    lifecycleStatusDraft: "enabled" | "disabled" | "all"
    setLifecycleStatusDraft: SetState<"enabled" | "disabled" | "all">
    revisionTimingDraft: "current" | "future" | "all"
    setRevisionTimingDraft: SetState<"current" | "future" | "all">
}) {
    const prefix = idPrefix ?? "master-data-list-dictionary-list-toolbar"
    const panelId = React.useId()
    const hasChips = hasActiveFilters && appliedChips.length > 0

    return (
        <form
            onSubmit={(event) => {
                event.preventDefault()
                applyListFilters()
            }}
        >
            <ListToolbar
                search={
                    <ListSearchField
                        id={`${prefix}-search-input`}
                        searchInputRef={searchInputRef}
                        value={searchDraft}
                        onChange={setSearchDraft}
                        placeholder={searchPlaceholder}
                    />
                }
                filters={
                    <Button
                        id={`${prefix}-filter-trigger`}
                        type="button"
                        variant="outline"
                        aria-expanded={filterPanelOpen}
                        aria-controls={panelId}
                        onClick={() => setFilterPanelOpen((open) => !open)}
                    >
                        <FilterIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        更多筛选
                        {hasStructuredListFilters ? (
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
                }
                secondary={
                    hasChips || filterPanelOpen ? (
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
                                        id={`${prefix}-clear-filters`}
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        onClick={clearAllFilters}
                                    >
                                        清空全部
                                    </Button>
                                </div>
                            ) : null}
                            {filterPanelOpen ? (
                                <div
                                    id={panelId}
                                    className="flex w-full flex-col gap-3 border-t pt-3"
                                    aria-label={`${countLabel}更多筛选条件`}
                                >
                                    <FixedOptionRadioFilter
                                        label="启停"
                                        value={lifecycleStatusDraft}
                                        onValueChange={setLifecycleStatusDraft}
                                        options={LIFECYCLE_RADIO_FILTER_OPTIONS}
                                        aria-label={
                                            masterDataCopy.filterLifecycleAria
                                        }
                                    />
                                    <FixedOptionRadioFilter
                                        label="版本"
                                        value={revisionTimingDraft}
                                        onValueChange={setRevisionTimingDraft}
                                        options={
                                            REVISION_TIMING_RADIO_FILTER_OPTIONS
                                        }
                                        aria-label={
                                            masterDataCopy.filterVersionAria
                                        }
                                    />
                                    <div className="flex flex-col gap-3 border-t pt-3 sm:flex-row sm:items-center sm:justify-between">
                                        <p className="text-xs text-muted-foreground">
                                            将同时应用上方关键词和以下筛选条件；结果也用于导出。
                                        </p>
                                        <div className="flex flex-wrap items-center gap-2 sm:justify-end">
                                            <Button
                                                id={`${prefix}-reset`}
                                                type="button"
                                                variant="ghost"
                                                onClick={resetMoreFilters}
                                            >
                                                重置更多条件
                                            </Button>
                                            <Button
                                                id={`${prefix}-apply`}
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
