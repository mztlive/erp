"use client"

import {
    InputGroup,
    InputGroupAddon,
    InputGroupButton,
    InputGroupInput,
} from "@/components/ui/input-group"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { cn } from "@/lib/utils"

import type { WorkspaceUrlState } from "../lib/url-state"
import type {
    WorkspaceFamilyFilter,
    WorkspaceMetric,
    WorkspaceMetricKey,
    WorkspaceSort,
} from "../types"

const FAMILIES: readonly (WorkspaceFamilyFilter | undefined)[] = [
    undefined,
    "approval",
    "fulfillment",
    "finance",
    "exception",
]

const FAMILY_LABEL: Record<string, string> = {
    approval: "审批",
    fulfillment: "履约",
    finance: "财务",
    exception: "集成",
}

const SORT_OPTIONS: readonly { value: WorkspaceSort; label: string }[] = [
    { value: "priority_due", label: "超期与优先级" },
    { value: "due_asc", label: "截止时间" },
    { value: "created_desc", label: "进入时间" },
]

/**
 * 口径胶囊、类型页签、搜索与排序。只筛选左列，不改变指标数量口径。
 */
export function WorkspaceFilterBar({
    urlState,
    metrics,
    activeMetric,
    searchDraft,
    onSearchDraftChange,
    onMetricClick,
    onFamilyChange,
    onSortChange,
    onSearch,
}: {
    urlState: WorkspaceUrlState
    metrics: readonly WorkspaceMetric[]
    activeMetric: WorkspaceMetricKey
    searchDraft: string
    onSearchDraftChange: (value: string) => void
    onMetricClick: (key: WorkspaceMetricKey) => void
    onFamilyChange: (family?: WorkspaceFamilyFilter) => void
    onSortChange: (sort: WorkspaceSort) => void
    onSearch: () => void
}) {
    const visibleMetrics = metrics.filter((metric) => metric.visible)
    const familyValue = urlState.family ?? "all"

    return (
        <div className="flex flex-col gap-2 border-b border-border/30 px-3 py-2">
            <div className="flex flex-col gap-2 lg:flex-row lg:items-center lg:justify-between">
                <ToggleGroup
                    value={[activeMetric]}
                    onValueChange={(values) => {
                        const next = values[0]
                        if (!next) return
                        onMetricClick(next as WorkspaceMetricKey)
                    }}
                    variant="outline"
                    size="sm"
                    spacing={0}
                    aria-label="待办筛选"
                >
                    {visibleMetrics.map((metric) => (
                        <ToggleGroupItem key={metric.key} value={metric.key}>
                            {metric.label}
                            <span
                                className={cn(
                                    "num",
                                    metric.tone === "destructive" &&
                                        metric.count > 0 &&
                                        "text-destructive",
                                )}
                            >
                                {metric.count}
                            </span>
                        </ToggleGroupItem>
                    ))}
                </ToggleGroup>
                <form
                    className="flex flex-wrap items-center gap-2"
                    onSubmit={(event) => {
                        event.preventDefault()
                        onSearch()
                    }}
                >
                    <InputGroup className="w-52 max-w-full">
                        <InputGroupInput
                            value={searchDraft}
                            onChange={(event) =>
                                onSearchDraftChange(event.target.value)
                            }
                            placeholder="搜索单号或往来方"
                            aria-label="搜索待办"
                        />
                        <InputGroupAddon align="inline-end">
                            <InputGroupButton type="submit">
                                搜索
                            </InputGroupButton>
                        </InputGroupAddon>
                    </InputGroup>
                    <NativeSelect
                        value={urlState.sort}
                        aria-label="排序"
                        onChange={(event) =>
                            onSortChange(event.target.value as WorkspaceSort)
                        }
                    >
                        {SORT_OPTIONS.map((option) => (
                            <NativeSelectOption
                                key={option.value}
                                value={option.value}
                            >
                                {option.label}
                            </NativeSelectOption>
                        ))}
                    </NativeSelect>
                </form>
            </div>
            <ToggleGroup
                value={[familyValue]}
                onValueChange={(values) => {
                    const next = values[0]
                    if (!next) return
                    onFamilyChange(
                        next === "all"
                            ? undefined
                            : (next as WorkspaceFamilyFilter),
                    )
                }}
                variant="outline"
                size="sm"
                spacing={0}
                aria-label="任务类型"
            >
                {FAMILIES.map((family) => {
                    const value = family ?? "all"
                    return (
                        <ToggleGroupItem key={value} value={value}>
                            {family ? FAMILY_LABEL[family] : "全部"}
                        </ToggleGroupItem>
                    )
                })}
            </ToggleGroup>
        </div>
    )
}
