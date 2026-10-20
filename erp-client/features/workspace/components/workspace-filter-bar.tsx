"use client"

import { CopyNavLink } from "@/features/approval-copies/page"
import type { ReactNode } from "react"
import { ChevronDownIcon, SearchIcon, XIcon } from "lucide-react"

import { toAutomationIdSegment } from "@/lib/automation-id"

import { listWorkspaceFilterStatusText } from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
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
import {
    WorkspaceViewNav,
    WorkspaceViewNavItem,
} from "@/components/business/workspace-view-nav"

import type { WorkspaceUrlState } from "../lib/url-state"
import type {
    WorkspaceMetric,
    WorkspaceMetricKey,
    WorkspaceSort,
} from "../types"

const SORT_OPTIONS: readonly { value: WorkspaceSort; label: string }[] = [
    { value: "priority_due", label: "超期优先，再按优先级" },
    { value: "due_asc", label: "截止时间 · 最早优先" },
    { value: "created_desc", label: "进入时间 · 最新优先" },
]

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
            (metric.key === "inbox" ||
                metric.key === "managed" ||
                metric.key === "started"),
    )
    const scopeKey =
        activeMetric === "started" || activeMetric === "managed"
            ? activeMetric
            : "inbox"

    return (
        <WorkspaceViewNav>
            {visibleMetrics.map((metric) => (
                <WorkspaceViewNavItem
                    key={metric.key}
                    id={`workspace-queue-scope-${toAutomationIdSegment(metric.key)}`}
                    active={metric.key === scopeKey}
                    aria-label={`${metric.label} ${metric.count} 项`}
                    onClick={() => onMetricClick(metric.key)}
                >
                    <span>{metric.label}</span>
                    <span className="num ml-1 text-xs text-muted-foreground">
                        {metric.count}
                    </span>
                </WorkspaceViewNavItem>
            ))}
            <CopyNavLink />
        </WorkspaceViewNav>
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
                            <NativeCheckbox
                                id={"workspace-queue-scope-" + metric.key}
                                checked={activeMetric === metric.key}
                                onCheckedChange={() =>
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
                                loading={loading}
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
