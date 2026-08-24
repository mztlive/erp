"use client"

import { SearchIcon } from "lucide-react"

import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
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
 * 贴画布的口径数字。数字本身是筛选，不是统计卡。
 */
export function WorkspaceOverviewBar({
    metrics,
    activeMetric,
    onMetricClick,
    className,
}: {
    metrics: readonly WorkspaceMetric[]
    activeMetric: WorkspaceMetricKey
    onMetricClick: (key: WorkspaceMetricKey) => void
    className?: string
}) {
    const visibleMetrics = metrics.filter((metric) => metric.visible)

    return (
        <div
            role="group"
            aria-label="待办筛选"
            className={cn(
                "flex shrink-0 flex-wrap gap-x-8 gap-y-3 border-b border-border/30 pb-4",
                className,
            )}
        >
            {visibleMetrics.map((metric) => {
                const active = metric.key === activeMetric
                const danger =
                    metric.tone === "destructive" && metric.count > 0
                return (
                    <button
                        key={metric.key}
                        type="button"
                        aria-pressed={active}
                        aria-label={`${metric.label} ${metric.count}`}
                        onClick={() => onMetricClick(metric.key)}
                        className="flex min-w-14 flex-col items-start gap-1 text-left"
                    >
                        <span
                            className={cn(
                                "num text-3xl leading-none font-semibold tracking-tight",
                                danger
                                    ? "text-destructive"
                                    : active
                                      ? "text-foreground"
                                      : "text-muted-foreground",
                            )}
                        >
                            {metric.count}
                        </span>
                        <span
                            className={cn(
                                "border-b pb-0.5 text-xs",
                                active
                                    ? "border-foreground font-medium text-foreground"
                                    : "border-transparent text-muted-foreground",
                            )}
                        >
                            {metric.label}
                        </span>
                    </button>
                )
            })}
        </div>
    )
}

/**
 * 队列类型。和待办列表同一列，不跨到作业面。
 */
export function WorkspaceFamilyNav({
    urlState,
    onFamilyChange,
}: {
    urlState: WorkspaceUrlState
    onFamilyChange: (family?: WorkspaceFamilyFilter) => void
}) {
    const familyValue = urlState.family ?? "all"

    return (
        <div
            role="group"
            aria-label="任务类型"
            className="flex flex-wrap items-center gap-1"
        >
            {FAMILIES.map((family) => {
                const value = family ?? "all"
                const active = familyValue === value
                return (
                    <button
                        key={value}
                        type="button"
                        aria-pressed={active}
                        onClick={() =>
                            onFamilyChange(
                                family === undefined ? undefined : family,
                            )
                        }
                        className={cn(
                            "h-8 px-1.5 text-sm",
                            active
                                ? "font-medium text-foreground"
                                : "text-muted-foreground hover:text-foreground",
                        )}
                    >
                        {family ? FAMILY_LABEL[family] : "全部"}
                    </button>
                )
            })}
        </div>
    )
}

/**
 * 队列内搜索与排序。Enter 提交关键词，不另放搜索按钮。
 */
export function WorkspaceQueueToolbar({
    urlState,
    searchDraft,
    onSearchDraftChange,
    onSortChange,
    onSearch,
    stacked = false,
}: {
    urlState: WorkspaceUrlState
    searchDraft: string
    onSearchDraftChange: (value: string) => void
    onSortChange: (sort: WorkspaceSort) => void
    onSearch: () => void
    stacked?: boolean
}) {
    return (
        <form
            className={cn(
                "flex gap-2",
                stacked ? "flex-col" : "max-w-xl flex-row items-center",
            )}
            onSubmit={(event) => {
                event.preventDefault()
                onSearch()
            }}
        >
            <InputGroup className="min-w-0 flex-1">
                <InputGroupAddon>
                    <SearchIcon aria-hidden="true" />
                </InputGroupAddon>
                <InputGroupInput
                    value={searchDraft}
                    onChange={(event) =>
                        onSearchDraftChange(event.target.value)
                    }
                    placeholder="单号或往来方"
                    aria-label="搜索待办"
                />
            </InputGroup>
            <NativeSelect
                size="sm"
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
    )
}
