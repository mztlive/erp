"use client"

import type { ComponentProps } from "react"
import { ChevronDownIcon, SearchIcon } from "lucide-react"

import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuGroup,
    DropdownMenuRadioGroup,
    DropdownMenuRadioItem,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupButton,
    InputGroupInput,
} from "@/components/ui/input-group"
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

/** 文字导航项。选中靠字重，不用胶囊底。 */
function WorkspaceTextNavButton({
    active,
    children,
    ...props
}: ComponentProps<"button"> & { active: boolean }) {
    return (
        <button
            {...props}
            type="button"
            aria-pressed={active}
            className={cn(
                "h-8 px-1.5 text-sm",
                active
                    ? "font-medium text-foreground"
                    : "text-muted-foreground hover:text-foreground",
            )}
        >
            {children}
        </button>
    )
}

/**
 * 队列口径切换。只切换待办范围，不展示统计数字。
 */
export function WorkspaceQueueScopeNav({
    metrics,
    activeMetric,
    onMetricClick,
}: {
    metrics: readonly WorkspaceMetric[]
    activeMetric: WorkspaceMetricKey
    onMetricClick: (key: WorkspaceMetricKey) => void
}) {
    const visibleMetrics = metrics.filter((metric) => metric.visible)

    return (
        <div
            role="group"
            aria-label="待办筛选"
            className="flex flex-wrap items-center gap-1"
        >
            {visibleMetrics.map((metric) => (
                <WorkspaceTextNavButton
                    key={metric.key}
                    active={metric.key === activeMetric}
                    onClick={() => onMetricClick(metric.key)}
                >
                    {metric.label}
                </WorkspaceTextNavButton>
            ))}
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
                return (
                    <WorkspaceTextNavButton
                        key={value}
                        active={familyValue === value}
                        onClick={() =>
                            onFamilyChange(
                                family === undefined ? undefined : family,
                            )
                        }
                    >
                        {family ? FAMILY_LABEL[family] : "全部"}
                    </WorkspaceTextNavButton>
                )
            })}
        </div>
    )
}

/**
 * 队列内搜索与排序。同属一条搜索栏，Enter 提交关键词。
 */
export function WorkspaceQueueToolbar({
    urlState,
    searchDraft,
    onSearchDraftChange,
    onSortChange,
    onSearch,
}: {
    urlState: WorkspaceUrlState
    searchDraft: string
    onSearchDraftChange: (value: string) => void
    onSortChange: (sort: WorkspaceSort) => void
    onSearch: () => void
}) {
    const sortLabel =
        SORT_OPTIONS.find((option) => option.value === urlState.sort)?.label ??
        "排序"

    return (
        <form
            onSubmit={(event) => {
                event.preventDefault()
                onSearch()
            }}
        >
            <InputGroup className="min-w-0">
                <InputGroupAddon>
                    <SearchIcon aria-hidden="true" />
                </InputGroupAddon>
                <InputGroupInput
                    value={searchDraft}
                    onChange={(event) =>
                        onSearchDraftChange(event.target.value)
                    }
                    placeholder="搜索单号或往来方"
                    aria-label="搜索待办"
                />
                <InputGroupAddon
                    align="inline-end"
                    className="border-l border-border/60 pl-1"
                >
                    <DropdownMenu>
                        <DropdownMenuTrigger
                            render={
                                <InputGroupButton
                                    variant="ghost"
                                    size="xs"
                                    aria-label={`排序：${sortLabel}`}
                                />
                            }
                        >
                            排序
                            <ChevronDownIcon data-icon="inline-end" />
                        </DropdownMenuTrigger>
                        <DropdownMenuContent
                            align="end"
                            className="w-auto min-w-40"
                        >
                            <DropdownMenuGroup>
                                <DropdownMenuRadioGroup
                                    value={urlState.sort}
                                    onValueChange={(value) => {
                                        if (value) {
                                            onSortChange(value as WorkspaceSort)
                                        }
                                    }}
                                >
                                    {SORT_OPTIONS.map((option) => (
                                        <DropdownMenuRadioItem
                                            key={option.value}
                                            value={option.value}
                                        >
                                            {option.label}
                                        </DropdownMenuRadioItem>
                                    ))}
                                </DropdownMenuRadioGroup>
                            </DropdownMenuGroup>
                        </DropdownMenuContent>
                    </DropdownMenu>
                </InputGroupAddon>
            </InputGroup>
        </form>
    )
}
