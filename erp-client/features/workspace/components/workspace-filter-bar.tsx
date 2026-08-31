"use client"

import type { ComponentProps } from "react"
import { ChevronDownIcon, SearchIcon } from "lucide-react"

import { toAutomationIdSegment } from "@/lib/automation-id"

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
    WorkspaceFamilyCounts,
    WorkspaceMetric,
    WorkspaceMetricKey,
    WorkspaceSort,
} from "../types"

const FAMILIES: readonly (WorkspaceFamilyFilter | undefined)[] = [
    undefined,
    "approval",
    "procurement",
    "fulfillment",
    "finance",
    "exception",
]

const FAMILY_LABEL: Record<string, string> = {
    approval: "审批",
    procurement: "采购",
    fulfillment: "履约",
    finance: "财务",
    exception: "异常",
}

const SORT_OPTIONS: readonly { value: WorkspaceSort; label: string }[] = [
    { value: "priority_due", label: "超期与优先级" },
    { value: "due_asc", label: "截止时间" },
    { value: "created_desc", label: "进入时间" },
]

/** 文字导航项。选中态同时使用字重与下划线，不能只依赖颜色。 */
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
                "relative h-8 rounded-sm px-1.5 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
                active
                    ? "font-medium text-foreground after:absolute after:inset-x-1.5 after:bottom-0 after:h-0.5 after:rounded-full after:bg-foreground"
                    : "text-muted-foreground hover:text-foreground",
            )}
        >
            {children}
        </button>
    )
}

/**
 * 队列口径切换。数量直接使用服务端指标，禁止对已加载条目求和。
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
                    id={`workspace-queue-scope-${toAutomationIdSegment(metric.key)}`}
                    active={metric.key === activeMetric}
                    aria-label={`${metric.label} ${metric.count} 项`}
                    onClick={() => onMetricClick(metric.key)}
                >
                    <span>{metric.label}</span>
                    <span className="num ml-1 text-xs text-muted-foreground">
                        {metric.count}
                    </span>
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
    counts,
    onFamilyChange,
}: {
    urlState: WorkspaceUrlState
    counts?: WorkspaceFamilyCounts
    onFamilyChange: (family?: WorkspaceFamilyFilter) => void
}) {
    const familyValue = urlState.family ?? "all"
    const allCount = counts
        ? Object.values(counts).reduce((total, count) => total + count, 0)
        : undefined

    return (
        <div
            role="group"
            aria-label="任务类型"
            className="flex flex-wrap items-center gap-1"
        >
            {FAMILIES.map((family) => {
                const value = family ?? "all"
                const count = family ? counts?.[family] : allCount
                return (
                    <WorkspaceTextNavButton
                        key={value}
                        id={`workspace-family-nav-${toAutomationIdSegment(value)}`}
                        active={familyValue === value}
                        aria-label={
                            count == null
                                ? undefined
                                : `${family ? FAMILY_LABEL[family] : "全部"} ${count} 项`
                        }
                        onClick={() =>
                            onFamilyChange(
                                family === undefined ? undefined : family,
                            )
                        }
                    >
                        <span>{family ? FAMILY_LABEL[family] : "全部"}</span>
                        {count == null ? null : (
                            <span className="num ml-1 text-xs text-muted-foreground">
                                {count}
                            </span>
                        )}
                    </WorkspaceTextNavButton>
                )
            })}
        </div>
    )
}

/**
 * 队列内搜索与排序。同属一条搜索栏，Enter 提交关键词。
 * 我发起的审批不提供待办排序，只保留检索。
 */
export function WorkspaceQueueToolbar({
    urlState,
    searchDraft,
    onSearchDraftChange,
    onSortChange,
    onSearch,
    showSort = true,
    searchAriaLabel = "搜索待办",
}: {
    urlState: WorkspaceUrlState
    searchDraft: string
    onSearchDraftChange: (value: string) => void
    onSortChange: (sort: WorkspaceSort) => void
    onSearch: () => void
    showSort?: boolean
    searchAriaLabel?: string
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
                    id="workspace-queue-toolbar-search-input"
                    value={searchDraft}
                    onChange={(event) =>
                        onSearchDraftChange(event.target.value)
                    }
                    placeholder="搜索单号或往来方"
                    aria-label={searchAriaLabel}
                />
                {showSort ? (
                    <InputGroupAddon
                        align="inline-end"
                        className="border-l border-border/60 pl-1"
                    >
                        <DropdownMenu>
                            <DropdownMenuTrigger
                                id="workspace-queue-toolbar-sort-trigger"
                                render={
                                    <InputGroupButton
                                        id="workspace-queue-toolbar-sort-trigger"
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
                                                onSortChange(
                                                    value as WorkspaceSort,
                                                )
                                            }
                                        }}
                                    >
                                        {SORT_OPTIONS.map((option) => (
                                            <DropdownMenuRadioItem
                                                key={option.value}
                                                id={`workspace-queue-toolbar-sort-option-${toAutomationIdSegment(option.value)}`}
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
                ) : null}
            </InputGroup>
        </form>
    )
}
