"use client"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { cn } from "@/lib/utils"

import type { WorkspaceUrlState } from "../lib/url-state"
import type { WorkspaceFamilyFilter, WorkspaceSort } from "../types"

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
 * 类型页签、搜索与排序。只筛选左列，不改变指标数量口径。
 */
export function WorkspaceFilterBar({
    urlState,
    searchDraft,
    onSearchDraftChange,
    onFamilyChange,
    onSortChange,
    onSearch,
}: {
    urlState: WorkspaceUrlState
    searchDraft: string
    onSearchDraftChange: (value: string) => void
    onFamilyChange: (family?: WorkspaceFamilyFilter) => void
    onSortChange: (sort: WorkspaceSort) => void
    onSearch: () => void
}) {
    return (
        <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
            <div
                className="flex flex-wrap gap-1"
                role="tablist"
                aria-label="任务类型"
            >
                {FAMILIES.map((family) => {
                    const active = urlState.family === family
                    const label = family ? FAMILY_LABEL[family] : "全部"
                    return (
                        <Button
                            key={family ?? "all"}
                            type="button"
                            size="xs"
                            variant={active ? "secondary" : "ghost"}
                            className={cn(active && "font-medium")}
                            aria-pressed={active}
                            onClick={() => onFamilyChange(family)}
                        >
                            {label}
                        </Button>
                    )
                })}
            </div>
            <div className="flex flex-wrap items-center gap-2">
                <Input
                    value={searchDraft}
                    onChange={(event) =>
                        onSearchDraftChange(event.target.value)
                    }
                    placeholder="搜索单号或往来方"
                    aria-label="搜索待办"
                    className="h-8 w-48"
                />
                <Button
                    type="button"
                    size="xs"
                    variant="outline"
                    onClick={onSearch}
                >
                    搜索
                </Button>
                <select
                    className="h-8 rounded-md border border-input bg-background px-2 text-sm"
                    value={urlState.sort}
                    aria-label="排序"
                    onChange={(event) =>
                        onSortChange(event.target.value as WorkspaceSort)
                    }
                >
                    {SORT_OPTIONS.map((option) => (
                        <option key={option.value} value={option.value}>
                            {option.label}
                        </option>
                    ))}
                </select>
            </div>
        </div>
    )
}
