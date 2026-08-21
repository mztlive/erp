"use client"

import * as React from "react"
import {
    ChevronDownIcon,
    FilterIcon,
    SearchIcon,
} from "lucide-react"

import { FilterChip, ListToolbar, OptionCombobox } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import type {
    BatchFilterKey,
    BatchObjectTypeDraft,
    BatchStatusDraft,
} from "@/features/import-opening/hooks/use-batch-list-filters"
import {
    BATCH_STATUS_LABEL,
    OBJECT_CODE_LABEL,
    type ImportBatchStatus,
    type ImportObjectCode,
} from "@/features/import-opening/types"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

export type BatchAppliedChip = Readonly<{
    key: BatchFilterKey
    label: string
}>

const OBJECT_FILTER_OPTIONS: ReadonlyArray<{
    value: BatchObjectTypeDraft
    label: string
}> = [
    { value: "all", label: "全部对象" },
    ...(Object.keys(OBJECT_CODE_LABEL) as ImportObjectCode[]).map((code) => ({
        value: code,
        label: OBJECT_CODE_LABEL[code],
    })),
]

const STATUS_FILTER_OPTIONS: ReadonlyArray<{
    value: BatchStatusDraft
    label: string
}> = [
    { value: "all", label: "全部状态" },
    ...(Object.keys(BATCH_STATUS_LABEL) as ImportBatchStatus[]).map(
        (status) => ({
            value: status,
            label: BATCH_STATUS_LABEL[status],
        }),
    ),
]

/**
 * 批次列表显式提交筛选区（docs/ui-filter-design.md §8.2 模板）：
 * 单一 form，收起态靠搜索框尾部提交箭头与 Enter，展开态只保留面板底部
 * 「应用全部筛选」，两条路径调用同一个 applyBatchFilters（§3.5）。
 */
export function BatchListToolbar({
    searchInputRef,
    searchDraft,
    setSearchDraft,
    hasActiveFilters,
    clearAllFilters,
    appliedChips,
    removeFilter,
    batchFilterPanelOpen,
    setBatchFilterPanelOpen,
    hasStructuredBatchFilters,
    applyBatchFilters,
    resetMoreFilters,
    objectTypeDraft,
    setObjectTypeDraft,
    statusDraft,
    setStatusDraft,
}: {
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: SetState<string>
    hasActiveFilters: boolean
    clearAllFilters: () => void
    appliedChips: readonly BatchAppliedChip[]
    removeFilter: (key: BatchFilterKey) => void
    batchFilterPanelOpen: boolean
    setBatchFilterPanelOpen: SetState<boolean>
    hasStructuredBatchFilters: boolean
    applyBatchFilters: () => void
    resetMoreFilters: () => void
    objectTypeDraft: BatchObjectTypeDraft
    setObjectTypeDraft: SetState<BatchObjectTypeDraft>
    statusDraft: BatchStatusDraft
    setStatusDraft: SetState<BatchStatusDraft>
}) {
    const panelId = React.useId()
    const hasChips = hasActiveFilters && appliedChips.length > 0

    return (
        <form
            onSubmit={(event) => {
                event.preventDefault()
                applyBatchFilters()
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
                            placeholder="批次号（精确/前缀匹配）"
                            aria-label="搜索批次"
                        />
                        
                    </InputGroup>
                }
                filters={
                    <Button
                        type="button"
                        variant="outline"
                        aria-expanded={batchFilterPanelOpen}
                        aria-controls={panelId}
                        onClick={() =>
                            setBatchFilterPanelOpen((open) => !open)
                        }
                    >
                        <FilterIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        更多筛选
                        {hasStructuredBatchFilters ? (
                            <Badge variant="info">已启用</Badge>
                        ) : null}
                        <ChevronDownIcon
                            data-icon="inline-end"
                            aria-hidden="true"
                            className={
                                batchFilterPanelOpen
                                    ? "rotate-180 transition-transform"
                                    : "transition-transform"
                            }
                        />
                    </Button>
                }
                secondary={
                    hasChips || batchFilterPanelOpen ? (
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
                            {batchFilterPanelOpen ? (
                                <div
                                    id={panelId}
                                    className="flex w-full flex-col gap-3 border-t pt-3"
                                    aria-label="导入批次更多筛选条件"
                                >
                                    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                对象集合
                                            </span>
                                            <OptionCombobox
                                                className="w-full"
                                                value={objectTypeDraft}
                                                onValueChange={(value) =>
                                                    setObjectTypeDraft(
                                                        (value ??
                                                            "all") as BatchObjectTypeDraft,
                                                    )
                                                }
                                                options={OBJECT_FILTER_OPTIONS}
                                                aria-label="对象集合"
                                                placeholder="全部对象"
                                                searchPlaceholder="搜索对象名称"
                                                allowClear={false}
                                            />
                                        </div>
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                批次状态
                                            </span>
                                            <OptionCombobox
                                                className="w-full"
                                                value={statusDraft}
                                                onValueChange={(value) =>
                                                    setStatusDraft(
                                                        (value ??
                                                            "all") as BatchStatusDraft,
                                                    )
                                                }
                                                options={STATUS_FILTER_OPTIONS}
                                                aria-label="批次状态"
                                                placeholder="全部状态"
                                                searchPlaceholder="搜索状态名称"
                                                allowClear={false}
                                            />
                                        </div>
                                    </div>
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
