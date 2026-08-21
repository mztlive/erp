"use client"

import * as React from "react"
import {
    ChevronDownIcon,
    FilterIcon,
    SearchIcon,
} from "lucide-react"

import {
    FilterChip,
    ListToolbar,
    OptionCombobox,
    surfacePanelClassName,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import type { MallSyncViewName } from "@/features/mall-sync/types"
import { MAPPING_TYPE_LABEL, VIEW_LABEL } from "@/features/mall-sync/types"
import { parseView, VIEWS } from "@/features/mall-sync/lib/presentation"
import type {
    MallSyncAppliedChip,
    MallSyncFilterKey,
    MallSyncMappingTypeDraft,
} from "@/features/mall-sync/pages/hooks/use-mall-sync-url-state"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

const MAPPING_TYPE_OPTIONS: ReadonlyArray<{
    value: MallSyncMappingTypeDraft
    label: string
}> = [
    { value: "all", label: "全部映射类型" },
    ...(
        Object.entries(MAPPING_TYPE_LABEL) as Array<
            [Exclude<MallSyncMappingTypeDraft, "all">, string]
        >
    ).map(([value, label]) => ({ value, label })),
]

type MallSyncViewToolbarProps = {
    view: MallSyncViewName
    onViewChange: (next: MallSyncViewName) => void
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: (value: string) => void
    mappingTypeDraft: MallSyncMappingTypeDraft
    setMappingTypeDraft: SetState<MallSyncMappingTypeDraft>
    panelOpen: boolean
    setPanelOpen: SetState<boolean>
    hasStructuredFilters: boolean
    hasActiveFilters: boolean
    appliedChips: readonly MallSyncAppliedChip[]
    removeFilter: (key: MallSyncFilterKey) => void
    applyFilters: () => void
    resetMoreFilters: () => void
    clearAllFilters: () => void
}

export function MallSyncViewToolbar({
    view,
    onViewChange,
    searchInputRef,
    searchDraft,
    setSearchDraft,
    mappingTypeDraft,
    setMappingTypeDraft,
    panelOpen,
    setPanelOpen,
    hasStructuredFilters,
    hasActiveFilters,
    appliedChips,
    removeFilter,
    applyFilters,
    resetMoreFilters,
    clearAllFilters,
}: MallSyncViewToolbarProps) {
    const panelId = React.useId()
    const hasChips = hasActiveFilters && appliedChips.length > 0

    return (
        <div
            className={`${surfacePanelClassName} sticky top-0 z-10 space-y-2.5 px-3 py-2.5`}
        >
            <Tabs
                value={view}
                onValueChange={(v) => {
                    onViewChange(parseView(v))
                }}
            >
                <TabsList
                    variant="line"
                    className="w-full justify-start overflow-x-auto"
                >
                    {VIEWS.map((v) => (
                        <TabsTrigger key={v} value={v}>
                            {VIEW_LABEL[v]}
                        </TabsTrigger>
                    ))}
                </TabsList>
            </Tabs>
            <form
                onSubmit={(event) => {
                    event.preventDefault()
                    applyFilters()
                }}
            >
                <ListToolbar
                    aria-label="商城同步筛选"
                    search={
                        <InputGroup>
                            <InputGroupAddon>
                                <SearchIcon aria-hidden="true" />
                            </InputGroupAddon>
                            <InputGroupInput
                                ref={searchInputRef}
                                placeholder={
                                    view === "snapshots" || view === "mapping"
                                        ? "商城单号 / ERP 单号 / 任务号"
                                        : view === "jobs"
                                          ? "任务号"
                                          : "搜索仅对来源数据、同步任务与映射任务生效"
                                }
                                value={searchDraft}
                                onChange={(e) =>
                                    setSearchDraft(e.target.value)
                                }
                                aria-label="搜索"
                            />
                            
                        </InputGroup>
                    }
                    filters={
                        view === "mapping" ? (
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
                        ) : undefined
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
                                        aria-label="商城同步更多筛选条件"
                                    >
                                        {view === "mapping" ? (
                                            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
                                                <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                                    <span className="text-muted-foreground">
                                                        映射类型
                                                    </span>
                                                    <OptionCombobox
                                                        className="w-full"
                                                        value={mappingTypeDraft}
                                                        onValueChange={(
                                                            value,
                                                        ) =>
                                                            setMappingTypeDraft(
                                                                (value ??
                                                                    "all") as MallSyncMappingTypeDraft,
                                                            )
                                                        }
                                                        options={
                                                            MAPPING_TYPE_OPTIONS
                                                        }
                                                        placeholder="全部映射类型"
                                                        searchPlaceholder="搜索映射类型"
                                                        aria-label="映射类型"
                                                    />
                                                </div>
                                            </div>
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
        </div>
    )
}
