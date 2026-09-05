"use client"

import {
    FixedOptionRadioFilter,
    surfacePanelClassName,
} from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    listWorkspaceFilterStatusText,
    type ListWorkspaceFilterChip,
} from "@/components/business/list-workspace"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { cn } from "@/lib/utils"

const prefix = "card-contracts-funds-review-filter"

const SCOPE_OPTIONS = [
    { value: "mine" as const, label: "我的待办" },
    { value: "history" as const, label: "处理历史" },
]

const TYPE_OPTIONS = [
    { value: "all" as const, label: "全部类型" },
    { value: "opening" as const, label: "期初" },
    { value: "delta" as const, label: "同步差额" },
]

const DUE_OPTIONS = [
    { value: "all" as const, label: "全部时限" },
    { value: "today" as const, label: "今日到期" },
    { value: "overdue" as const, label: "已超期" },
]

const STATUS_OPTIONS = [
    { value: "OPEN" as const, label: "待处理" },
    { value: "COMPLETED" as const, label: "已完成" },
    { value: "CLOSED" as const, label: "已关闭" },
]

export function QueueFilterToolbar({
    scope,
    type,
    due,
    status,
    q,
    searchInput,
    onSearchInputChange,
    onApplyFilters,
    autoNext,
    setAutoNext,
    replaceUrl,
    onClearAll,
    hasPendingChanges,
    resultCount,
    loading,
    failed,
}: {
    scope: "mine" | "history"
    type: "all" | "opening" | "delta"
    due: "all" | "today" | "overdue"
    status: "OPEN" | "COMPLETED" | "CLOSED"
    q: string | undefined
    searchInput: string
    onSearchInputChange: (value: string) => void
    onApplyFilters: () => void
    autoNext: boolean
    setAutoNext: (on: boolean) => void
    replaceUrl: (patch: Record<string, string | null | undefined>) => void
    onClearAll: () => void
    hasPendingChanges: boolean
    resultCount?: number
    loading: boolean
    failed: boolean
}) {
    const chips: ListWorkspaceFilterChip[] = []
    const queryText = q?.trim()
    if (queryText) chips.push({ key: "q", label: `搜索：${queryText}` })
    if (type !== "all") {
        chips.push({
            key: "type",
            label: `类型：${TYPE_OPTIONS.find((item) => item.value === type)?.label ?? type}`,
        })
    }
    if (due !== "all") {
        chips.push({
            key: "due",
            label: `时限：${DUE_OPTIONS.find((item) => item.value === due)?.label ?? due}`,
        })
    }
    if (status !== "OPEN") {
        chips.push({
            key: "status",
            label: `状态：${STATUS_OPTIONS.find((item) => item.value === status)?.label ?? status}`,
        })
    }

    return (
        <div className={cn(surfacePanelClassName, "sticky top-0 z-10")}>
            <ListWorkspaceFilterBar
                idPrefix={prefix}
                formAriaLabel="票款复核查询"
                onSubmit={onApplyFilters}
                search={
                    <ListSearchField
                        id={`${prefix}-search`}
                        value={searchInput}
                        onChange={onSearchInputChange}
                        placeholder="搜索单号 / 客户 / 往来主体"
                        aria-label="搜索复核队列"
                    />
                }
                commonFilters={
                    <>
                        <FixedOptionRadioFilter
                            idPrefix={`${prefix}-scope`}
                            label="责任范围"
                            variant="quiet"
                            value={scope}
                            onValueChange={(value) =>
                                replaceUrl({
                                    scope: value === "mine" ? null : value,
                                    status:
                                        value === "history"
                                            ? "COMPLETED"
                                            : null,
                                    queueContextId: null,
                                    currentWorkItemId: null,
                                })
                            }
                            options={SCOPE_OPTIONS}
                        />
                        <FixedOptionRadioFilter
                            idPrefix={`${prefix}-type`}
                            label="任务类型"
                            variant="quiet"
                            value={type}
                            onValueChange={(value) =>
                                replaceUrl({
                                    type: value === "all" ? null : value,
                                    currentWorkItemId: null,
                                })
                            }
                            options={TYPE_OPTIONS}
                        />
                        <FixedOptionRadioFilter
                            idPrefix={`${prefix}-due`}
                            label="到期时限"
                            variant="quiet"
                            value={due}
                            onValueChange={(value) =>
                                replaceUrl({
                                    due: value === "all" ? null : value,
                                    currentWorkItemId: null,
                                })
                            }
                            options={DUE_OPTIONS}
                        />
                        <FixedOptionRadioFilter
                            idPrefix={`${prefix}-status`}
                            label="队列范围"
                            variant="quiet"
                            value={status}
                            onValueChange={(value) =>
                                replaceUrl({
                                    status: value === "OPEN" ? null : value,
                                    scope: value === "OPEN" ? null : "history",
                                    currentWorkItemId: null,
                                })
                            }
                            options={STATUS_OPTIONS}
                        />
                    </>
                }
                resultStatus={listWorkspaceFilterStatusText({
                    loading,
                    failed,
                    resultCount,
                    noun: "条任务",
                    loadingLabel: "正在加载复核队列…",
                })}
                chips={chips}
                onClearChip={(key) => {
                    if (key === "q") {
                        onSearchInputChange("")
                        replaceUrl({ q: null, currentWorkItemId: null })
                        return
                    }
                    if (key === "type") {
                        replaceUrl({ type: null, currentWorkItemId: null })
                        return
                    }
                    if (key === "due") {
                        replaceUrl({ due: null, currentWorkItemId: null })
                        return
                    }
                    replaceUrl({
                        status: null,
                        scope: null,
                        currentWorkItemId: null,
                    })
                }}
                onClearAll={onClearAll}
                hasPendingChanges={hasPendingChanges}
                pendingHint="条件已修改，待查询"
                actions={
                    <div className="flex items-center gap-2">
                        <Label
                            htmlFor={`${prefix}-auto-next`}
                            className="text-muted-foreground"
                        >
                            自动下一项
                        </Label>
                        <Switch
                            id={`${prefix}-auto-next`}
                            checked={autoNext}
                            onCheckedChange={setAutoNext}
                        />
                    </div>
                }
            />
        </div>
    )
}
