"use client"

import type { ComponentProps, ReactNode } from "react"
import { ChevronDownIcon, SearchIcon, XIcon } from "lucide-react"

import { toAutomationIdSegment } from "@/lib/automation-id"

import { listWorkspaceFilterStatusText } from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupButton,
    InputGroupInput,
} from "@/components/ui/input-group"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuGroup,
    DropdownMenuRadioGroup,
    DropdownMenuRadioItem,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
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
    const visibleMetrics = metrics.filter(
        (metric) =>
            metric.visible &&
            (metric.key === "inbox" || metric.key === "started"),
    )

    return (
        <div
            role="group"
            aria-label="工作视图"
            className="flex flex-wrap items-center gap-x-8 gap-y-1"
        >
            {visibleMetrics.map((metric) => (
                <WorkspaceTextNavButton
                    key={metric.key}
                    id={`workspace-queue-scope-${toAutomationIdSegment(metric.key)}`}
                    active={
                        metric.key ===
                        (activeMetric === "started" ? "started" : "inbox")
                    }
                    aria-label={`${metric.key === "started" ? "我发起的审批" : metric.label} ${metric.count} 项`}
                    onClick={() => onMetricClick(metric.key)}
                >
                    <span>
                        {metric.key === "started"
                            ? "我发起的审批"
                            : metric.label}
                    </span>
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
        <div role="group" aria-label="任务类型">
            <DropdownMenu>
                <DropdownMenuTrigger
                    id="workspace-family-filter-trigger"
                    render={
                        <Button
                            type="button"
                            variant="outline"
                            className="min-w-32 justify-between"
                            aria-label={
                                "任务类型：" +
                                (FAMILY_LABEL[familyValue] ?? "全部")
                            }
                        />
                    }
                >
                    类型：{FAMILY_LABEL[familyValue] ?? "全部"}
                    <ChevronDownIcon aria-hidden="true" />
                </DropdownMenuTrigger>
                <DropdownMenuContent align="start">
                    <DropdownMenuGroup>
                        <DropdownMenuRadioGroup
                            value={familyValue}
                            onValueChange={(value) =>
                                onFamilyChange(
                                    value === "all"
                                        ? undefined
                                        : (value as WorkspaceFamilyFilter),
                                )
                            }
                        >
                            {FAMILIES.map((family) => {
                                const value = family ?? "all"
                                const count = family
                                    ? counts?.[family]
                                    : allCount
                                const label = family
                                    ? FAMILY_LABEL[family]
                                    : "全部"
                                return (
                                    <DropdownMenuRadioItem
                                        closeOnClick
                                        key={value}
                                        id={"workspace-family-nav-" + value}
                                        value={value}
                                        aria-label={
                                            label +
                                            (count == null
                                                ? ""
                                                : " " + count + " 项")
                                        }
                                    >
                                        {label}
                                        {count == null ? null : (
                                            <span className="num ml-auto pl-6 text-xs text-muted-foreground">
                                                {count}
                                            </span>
                                        )}
                                    </DropdownMenuRadioItem>
                                )
                            })}
                        </DropdownMenuRadioGroup>
                    </DropdownMenuGroup>
                </DropdownMenuContent>
            </DropdownMenu>
        </div>
    )
}

/** 超期、受阻属于待办的快捷条件，不与工作视图并列。 */
export function WorkspaceQueueStatusNav({
    metrics,
    activeMetric,
    onMetricClick,
}: {
    metrics: readonly WorkspaceMetric[]
    activeMetric: WorkspaceMetricKey
    onMetricClick: (key: WorkspaceMetricKey) => void
}) {
    return (
        <div
            role="group"
            aria-label="待办状态"
            className="flex flex-wrap items-center gap-4"
        >
            {metrics
                .filter(
                    (metric) =>
                        metric.visible &&
                        (metric.key === "overdue" || metric.key === "blocked"),
                )
                .map((metric) => {
                    const label =
                        metric.key === "overdue" ? "仅看超期" : "仅看受阻"
                    return (
                        <label
                            key={metric.key}
                            className="flex cursor-pointer items-center gap-2 whitespace-nowrap text-sm"
                            htmlFor={"workspace-queue-scope-" + metric.key}
                        >
                            <input
                                type="checkbox"
                                className="size-4 cursor-pointer accent-foreground"
                                id={"workspace-queue-scope-" + metric.key}
                                checked={activeMetric === metric.key}
                                onChange={() =>
                                    onMetricClick(
                                        activeMetric === metric.key
                                            ? "inbox"
                                            : metric.key,
                                    )
                                }
                            />
                            {label}
                            <span
                                className={cn(
                                    "num text-xs",
                                    metric.count > 0
                                        ? "text-warning-soft-foreground"
                                        : "text-muted-foreground",
                                )}
                            >
                                {metric.count}
                            </span>
                        </label>
                    )
                })}
        </div>
    )
}

/**
 * 队列内搜索与排序。Enter 与「查询」提交关键词。
 * 我发起的审批不提供待办排序，只保留检索。
 */
export function WorkspaceQueueToolbar({
    urlState,
    searchDraft,
    onSearchDraftChange,
    onSortChange,
    onSearch,
    onClearSearch,
    filters,
    showSort = true,
    searchAriaLabel = "搜索待办",
    resultCount,
    loading,
    failed,
    resultNoun = "条待办",
}: {
    urlState: WorkspaceUrlState
    searchDraft: string
    onSearchDraftChange: (value: string) => void
    onSortChange: (sort: WorkspaceSort) => void
    onSearch: () => void
    onClearSearch?: () => void
    filters?: ReactNode
    showSort?: boolean
    searchAriaLabel?: string
    resultCount?: number
    loading?: boolean
    failed?: boolean
    resultNoun?: string
}) {
    const sortLabel =
        SORT_OPTIONS.find((option) => option.value === urlState.sort)?.label ??
        "排序"
    const hasPendingChanges =
        searchDraft.trim() !== (urlState.query ?? "").trim()

    return (
        <form
            aria-label={showSort ? "待办查询" : "审批查询"}
            className="flex min-w-0 flex-col gap-2"
            onSubmit={(event) => {
                event.preventDefault()
                onSearch()
            }}
        >
            <div className="flex flex-wrap items-center gap-x-4 gap-y-3">
                <div className="w-full sm:w-80 xl:w-96">
                    <InputGroup>
                        <InputGroupAddon>
                            <InputGroupButton
                                id="workspace-queue-toolbar-query"
                                type="submit"
                                size="icon-xs"
                                aria-label="查询"
                            >
                                <SearchIcon aria-hidden="true" />
                            </InputGroupButton>
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
                        {onClearSearch && (searchDraft || urlState.query) ? (
                            <InputGroupAddon align="inline-end">
                                <InputGroupButton
                                    id="workspace-queue-toolbar-search-clear"
                                    size="icon-xs"
                                    aria-label="清除关键词"
                                    onClick={onClearSearch}
                                >
                                    <XIcon aria-hidden="true" />
                                </InputGroupButton>
                            </InputGroupAddon>
                        ) : null}
                    </InputGroup>
                </div>
                {filters}
                {showSort ? (
                    <div className="sm:ml-auto">
                        <DropdownMenu>
                            <DropdownMenuTrigger
                                id="workspace-queue-toolbar-sort-trigger"
                                render={
                                    <Button
                                        id="workspace-queue-toolbar-sort-trigger"
                                        type="button"
                                        variant="ghost"
                                        className="text-muted-foreground"
                                        aria-label={`排序：${sortLabel}`}
                                    />
                                }
                            >
                                排序：{sortLabel}
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
                                                closeOnClick
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
                    </div>
                ) : null}
            </div>
            {hasPendingChanges || resultCount !== undefined ? (
                <div
                    className="flex flex-wrap items-center gap-3 text-xs text-muted-foreground"
                    role="status"
                >
                    {resultCount !== undefined
                        ? listWorkspaceFilterStatusText({
                              loading,
                              failed,
                              resultCount,
                              noun: resultNoun,
                          })
                        : null}
                    {hasPendingChanges ? (
                        <span>关键词已修改，按回车查询</span>
                    ) : null}
                </div>
            ) : null}
        </form>
    )
}
