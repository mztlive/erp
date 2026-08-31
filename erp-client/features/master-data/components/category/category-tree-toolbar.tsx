"use client"

import * as React from "react"
import { SearchIcon } from "lucide-react"

import { FilterChip, ListToolbar } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type {
    CategoryTreeAppliedChip,
    CategoryTreeFilterKey,
} from "@/features/master-data/hooks/use-master-data-category-tree"

const LIFECYCLE_OPTIONS: ReadonlyArray<{
    value: "all" | "enabled" | "disabled"
    label: string
}> = [
    { value: "all", label: "全部" },
    { value: "enabled", label: masterDataCopy.lifecycleEnabled },
    { value: "disabled", label: masterDataCopy.lifecycleDisabled },
]

/** 分类树筛选条：搜索、启停快捷筛选与已生效 chip（docs/ui-filter-design.md §2.2/§3.1）。 */
export function CategoryTreeToolbar({
    idPrefix,
    searchInputRef,
    searchDraft,
    setSearchDraft,
    applyTreeFilters,
    lifecycleStatus,
    onLifecycleStatusChange,
    appliedChips,
    removeFilter,
    clearFilters,
}: {
    idPrefix?: string
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: (value: string) => void
    applyTreeFilters: () => void
    lifecycleStatus: "enabled" | "disabled" | "all"
    onLifecycleStatusChange: (value: "enabled" | "disabled" | "all") => void
    appliedChips: readonly CategoryTreeAppliedChip[]
    removeFilter: (key: CategoryTreeFilterKey) => void
    clearFilters: () => void
}) {
    const prefix = idPrefix ?? "master-data-category-tree-toolbar"
    const hasChips = appliedChips.length > 0

    return (
        <form
            className="border-b border-grid px-3 py-2.5"
            onSubmit={(event) => {
                event.preventDefault()
                applyTreeFilters()
            }}
        >
            <ListToolbar
                aria-label="分类树筛选"
                search={
                    <InputGroup>
                        <InputGroupAddon>
                            <SearchIcon aria-hidden />
                        </InputGroupAddon>
                        <InputGroupInput
                            id={`${prefix}-search`}
                            ref={searchInputRef}
                            value={searchDraft}
                            onChange={(e) => setSearchDraft(e.target.value)}
                            placeholder={masterDataCopy.categoryTreeSearch}
                            aria-label={masterDataCopy.categoryTreeSearch}
                        />
                    </InputGroup>
                }
                filters={
                    <div
                        role="group"
                        aria-label="生命周期筛选"
                        className="flex h-control max-w-full items-stretch overflow-x-auto rounded-lg border bg-muted/40 p-0.5 [&_[data-slot=button]]:h-full [&_[data-slot=button]]:min-h-0"
                    >
                        {LIFECYCLE_OPTIONS.map((option) => {
                            const active = lifecycleStatus === option.value
                            return (
                                <Button
                                    id={`${prefix}-lifecycle-${toAutomationIdSegment(option.value)}`}
                                    key={option.value}
                                    type="button"
                                    variant={active ? "secondary" : "ghost"}
                                    className={
                                        active
                                            ? "bg-card shadow-xs"
                                            : "shadow-none"
                                    }
                                    aria-pressed={active}
                                    onClick={() =>
                                        onLifecycleStatusChange(option.value)
                                    }
                                >
                                    {option.label}
                                </Button>
                            )
                        })}
                    </div>
                }
                secondary={
                    hasChips ? (
                        <div className="flex w-full flex-wrap items-center gap-2 border-t pt-3">
                            <span className="text-xs text-muted-foreground">
                                已筛选
                            </span>
                            {appliedChips.map((chip) => (
                                <FilterChip
                                    key={chip.key}
                                    id={`${prefix}-filter-${toAutomationIdSegment(chip.key)}`}
                                    label={chip.label}
                                    clearLabel={`移除${chip.label}`}
                                    onClear={() => removeFilter(chip.key)}
                                />
                            ))}
                            <Button
                                id={`${prefix}-clear-all`}
                                type="button"
                                variant="ghost"
                                size="xs"
                                onClick={clearFilters}
                            >
                                清空全部
                            </Button>
                        </div>
                    ) : undefined
                }
            />
        </form>
    )
}
