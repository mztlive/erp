"use client"

import * as React from "react"
import { SearchIcon, XIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"

/** 分类导航专用查询条；搜索提交与状态切换均保持可见反馈。 */
export function CategoryTreeToolbar({
    idPrefix,
    searchInputRef,
    searchDraft,
    setSearchDraft,
    applyTreeFilters,
    lifecycleStatus,
    onLifecycleStatusChange,
    clearFilters,
    clearSearch,
    filterActive,
    hasPendingChanges,
    loading,
}: {
    idPrefix: string
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: (value: string) => void
    applyTreeFilters: () => void
    lifecycleStatus: "enabled" | "disabled" | "all"
    onLifecycleStatusChange: (value: "enabled" | "disabled" | "all") => void
    clearSearch: () => void
    clearFilters: () => void
    filterActive: boolean
    hasPendingChanges: boolean
    loading: boolean
}) {
    return (
        <div className="space-y-3 pb-4">
            <form
                role="search"
                aria-label="搜索商品分类"
                className="relative flex gap-1.5"
                onSubmit={(event) => {
                    event.preventDefault()
                    applyTreeFilters()
                }}
            >
                <Input
                    id={`${idPrefix}-search`}
                    ref={searchInputRef}
                    value={searchDraft}
                    onChange={(event) => setSearchDraft(event.target.value)}
                    placeholder="搜索名称或代码"
                    aria-label="搜索分类名称或代码"
                    className="min-w-0 pr-8"
                />
                {searchDraft ? (
                    <Button
                        id={`${idPrefix}-clear`}
                        type="button"
                        variant="ghost"
                        size="icon-sm"
                        className="absolute top-1 right-11"
                        aria-label="清除搜索"
                        onClick={clearSearch}
                    >
                        <XIcon className="size-3.5" />
                    </Button>
                ) : null}
                <Button
                    id={`${idPrefix}-query`}
                    type="submit"
                    variant="outline"
                    size="icon"
                    aria-label="查询分类"
                >
                    <SearchIcon className="size-4" />
                </Button>
            </form>
            <div
                role="group"
                aria-label="分类状态"
                className="grid grid-cols-3 rounded-lg bg-muted/60 p-1"
            >
                {(
                    [
                        ["all", "全部"],
                        ["enabled", "启用"],
                        ["disabled", "停用"],
                    ] as const
                ).map(([value, label]) => (
                    <Button
                        id={`${idPrefix}-status-${value}`}
                        key={value}
                        type="button"
                        variant="ghost"
                        size="sm"
                        aria-pressed={lifecycleStatus === value}
                        className={
                            lifecycleStatus === value
                                ? "h-7 bg-background shadow-xs"
                                : "h-7 text-muted-foreground"
                        }
                        onClick={() => onLifecycleStatusChange(value)}
                    >
                        {label}
                    </Button>
                ))}
            </div>
            {filterActive || hasPendingChanges || loading ? (
                <div
                    className="flex items-center justify-between gap-2 text-xs text-muted-foreground"
                    role="status"
                >
                    <span>
                        {loading
                            ? "正在查询…"
                            : hasPendingChanges
                              ? "按回车应用搜索"
                              : "保留上级路径以便定位"}
                    </span>
                    {filterActive ? (
                        <Button
                            id={`${idPrefix}-reset`}
                            type="button"
                            variant="ghost"
                            size="sm"
                            className="h-auto px-1 py-0 text-xs"
                            onClick={clearFilters}
                        >
                            重置
                        </Button>
                    ) : null}
                </div>
            ) : null}
        </div>
    )
}
