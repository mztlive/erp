"use client"

import * as React from "react"
import { ChevronDownIcon, FilterIcon, SearchIcon } from "lucide-react"

import { FixedOptionRadioFilter, ListToolbar } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { ListSearchField } from "@/features/master-data/components/list/list-search-field"
import { ListToolbarCount } from "@/features/master-data/components/list/list-toolbar-actions"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import {
    LIFECYCLE_RADIO_FILTER_OPTIONS,
    REVISION_TIMING_RADIO_FILTER_OPTIONS,
} from "@/features/master-data/lib/list-filters"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

export function DictionaryListToolbar({
    searchInputRef,
    searchDraft,
    setSearchDraft,
    searchPlaceholder,
    countLabel,
    rowCount,
    hasActiveFilters,
    clearAllFilters,
    filterPanelOpen,
    setFilterPanelOpen,
    hasStructuredListFilters,
    applyListFilters,
    lifecycleStatusDraft,
    setLifecycleStatusDraft,
    revisionTimingDraft,
    setRevisionTimingDraft,
}: {
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: SetState<string>
    searchPlaceholder: string
    countLabel: string
    rowCount: number
    hasActiveFilters: boolean
    clearAllFilters: () => void
    filterPanelOpen: boolean
    setFilterPanelOpen: SetState<boolean>
    hasStructuredListFilters: boolean
    applyListFilters: () => void
    lifecycleStatusDraft: "enabled" | "disabled" | "all"
    setLifecycleStatusDraft: SetState<"enabled" | "disabled" | "all">
    revisionTimingDraft: "current" | "future" | "all"
    setRevisionTimingDraft: SetState<"current" | "future" | "all">
}) {
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
                        searchInputRef={searchInputRef}
                        value={searchDraft}
                        onChange={setSearchDraft}
                        placeholder={searchPlaceholder}
                    />
                }
                filters={
                    <>
                        {!filterPanelOpen ? (
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
                            aria-expanded={filterPanelOpen}
                            onClick={() => setFilterPanelOpen((open) => !open)}
                        >
                            <FilterIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            高级筛选
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
                    </>
                }
                secondary={
                    filterPanelOpen ? (
                        <div
                            className="flex w-full flex-col gap-3 rounded-lg border border-border bg-muted/30 px-3 py-3"
                            aria-label="列表筛选条件"
                        >
                            <FixedOptionRadioFilter
                                label="启停"
                                value={lifecycleStatusDraft}
                                onValueChange={setLifecycleStatusDraft}
                                options={LIFECYCLE_RADIO_FILTER_OPTIONS}
                                aria-label={masterDataCopy.filterLifecycleAria}
                            />
                            <FixedOptionRadioFilter
                                label="版本"
                                value={revisionTimingDraft}
                                onValueChange={setRevisionTimingDraft}
                                options={REVISION_TIMING_RADIO_FILTER_OPTIONS}
                                aria-label={masterDataCopy.filterVersionAria}
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
                        label={countLabel}
                        rowCount={rowCount}
                        hasActiveFilters={hasActiveFilters}
                        onClear={clearAllFilters}
                    />
                }
            />
        </form>
    )
}
