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
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import {
    SCOPE_LABELS,
    SCOPE_ORDER,
} from "@/features/customers/lib/filter-customers"
import type { DirectoryStatus } from "@/features/customers/lib/directory-url"
import type {
    CustomerAppliedChip,
    CustomerFilterKey,
} from "@/features/customers/hooks/use-customer-center-directory-state"
import type { CustomerScope } from "@/features/customers/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

const STATUS_RADIO_OPTIONS = [
    { value: "all", label: "全部" },
    { value: "active", label: "启用" },
    { value: "disabled", label: "停用" },
] as const

/**
 * 客户中心目录工具条（docs/ui-filter-design.md §8.2 模板）：
 * 单一 form；关键词 + 范围快捷筛选 + 「更多筛选」；已生效条件以 chip 展示。
 */
export function CustomerCenterDirectoryToolbar({
    searchInputRef,
    searchDraft,
    setSearchDraft,
    scope,
    onScopeChange,
    statusDraft,
    setStatusDraft,
    canReadAll,
    hasActiveFilters,
    appliedChips,
    removeFilter,
    panelOpen,
    setPanelOpen,
    hasStructuredFilters,
    applyFilters,
    resetMoreFilters,
    clearAllFilters,
}: {
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: SetState<string>
    scope: CustomerScope
    onScopeChange: (scope: CustomerScope) => void
    statusDraft: DirectoryStatus
    setStatusDraft: SetState<DirectoryStatus>
    canReadAll: boolean
    hasActiveFilters: boolean
    appliedChips: readonly CustomerAppliedChip[]
    removeFilter: (key: CustomerFilterKey) => void
    panelOpen: boolean
    setPanelOpen: SetState<boolean>
    hasStructuredFilters: boolean
    applyFilters: () => void
    resetMoreFilters: () => void
    clearAllFilters: () => void
}) {
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
                            id="customers-directory-search"
                            ref={searchInputRef}
                            data-slot="customer-search"
                            value={searchDraft}
                            onChange={(event) =>
                                setSearchDraft(event.target.value)
                            }
                            placeholder="客户名称、编号或信用代码"
                            aria-label="搜索客户"
                        />
                    </InputGroup>
                }
                filters={
                    <>
                        <div
                            role="group"
                            aria-label="客户范围"
                            className="flex h-control max-w-full items-stretch overflow-x-auto rounded-lg border bg-muted/40 p-0.5 [&_[data-slot=button]]:h-full [&_[data-slot=button]]:min-h-0"
                        >
                            {SCOPE_ORDER.filter(
                                (key) => key !== "all_authorized" || canReadAll,
                            ).map((key) => {
                                const active = scope === key
                                return (
                                    <Button
                                        id={`customers-directory-scope-${toAutomationIdSegment(key)}`}
                                        key={key}
                                        type="button"
                                        variant={active ? "secondary" : "ghost"}
                                        className={
                                            active
                                                ? "bg-card shadow-xs"
                                                : "shadow-none"
                                        }
                                        aria-pressed={active}
                                        onClick={() => onScopeChange(key)}
                                    >
                                        {SCOPE_LABELS[key]}
                                    </Button>
                                )
                            })}
                        </div>
                        <Button
                            id="customers-directory-more-filters-trigger"
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
                    </>
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
                                            id={`customers-directory-filter-chip-${toAutomationIdSegment(chip.key)}`}
                                            key={chip.key}
                                            label={chip.label}
                                            clearLabel={`移除${chip.label}`}
                                            onClear={() =>
                                                removeFilter(chip.key)
                                            }
                                        />
                                    ))}
                                    <Button
                                        id="customers-directory-clear-all"
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
                                    aria-label="客户目录筛选条件"
                                >
                                    <FixedOptionRadioFilter
                                        id="customers-directory-status"
                                        label="状态"
                                        value={statusDraft}
                                        onValueChange={setStatusDraft}
                                        options={STATUS_RADIO_OPTIONS}
                                    />
                                    <div className="flex flex-col gap-3 border-t pt-3 sm:flex-row sm:items-center sm:justify-between">
                                        <p className="text-xs text-muted-foreground">
                                            将同时应用上方关键词和以下筛选条件；结果也用于导出。
                                        </p>
                                        <div className="flex flex-wrap items-center gap-2 sm:justify-end">
                                            <Button
                                                id="customers-directory-reset-more"
                                                type="button"
                                                variant="ghost"
                                                onClick={resetMoreFilters}
                                            >
                                                重置更多条件
                                            </Button>
                                            <Button
                                                id="customers-directory-apply-filters"
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
