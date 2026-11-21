"use client"

import * as React from "react"
import { SearchIcon, XIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupButton,
    InputGroupInput,
} from "@/components/ui/input-group"
import { Kbd } from "@/components/ui/kbd"
import { cn } from "@/lib/utils"

/** 分类导航专用轻柔搜索条：无框浅灰底，与全站飞书风格工具栏保持同源。 */
export function CategoryTreeToolbar({
    idPrefix,
    searchInputRef,
    searchDraft,
    setSearchDraft,
    applyTreeFilters,
    clearFilters,
    clearSearch,
    filterActive,
    hasPendingChanges,
    loading,
    matchedCount,
}: {
    idPrefix: string
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: (value: string) => void
    applyTreeFilters: () => void
    clearSearch: () => void
    clearFilters: () => void
    filterActive: boolean
    hasPendingChanges: boolean
    loading: boolean
    matchedCount: number
}) {
    return (
        <div className="space-y-2 pb-3">
            <form
                role="search"
                aria-label="搜索商品分类"
                onSubmit={(event) => {
                    event.preventDefault()
                    applyTreeFilters()
                }}
            >
                <InputGroup
                    data-has-value={searchDraft.trim() ? "true" : undefined}
                    className={cn(
                        "border-transparent bg-muted/70 shadow-none hover:border-transparent hover:bg-muted",
                        "has-[[data-slot=input-group-control]:focus-visible]:border-primary/60 has-[[data-slot=input-group-control]:focus-visible]:bg-card",
                        searchDraft.trim() &&
                            "border-primary/25 bg-sidebar-accent/60 hover:bg-sidebar-accent/80",
                    )}
                >
                    <InputGroupAddon>
                        <SearchIcon
                            aria-hidden="true"
                            className={cn(
                                searchDraft.trim() &&
                                    "text-sidebar-accent-foreground/80",
                            )}
                        />
                    </InputGroupAddon>
                    <InputGroupInput
                        id={`${idPrefix}-search`}
                        ref={searchInputRef}
                        value={searchDraft}
                        onChange={(event) => setSearchDraft(event.target.value)}
                        placeholder="搜索名称或代码"
                        aria-label="搜索分类名称或代码"
                        className={cn(
                            searchDraft.trim() &&
                                "font-medium text-sidebar-accent-foreground",
                        )}
                    />
                    <InputGroupAddon align="inline-end" className="gap-0.5">
                        {searchDraft ? (
                            <InputGroupButton
                                id={`${idPrefix}-clear`}
                                type="button"
                                variant="ghost"
                                size="icon-xs"
                                aria-label="清除搜索"
                                title="清除"
                                className="rounded-full text-muted-foreground/75 hover:bg-background/80 hover:text-foreground"
                                onClick={() => {
                                    clearSearch()
                                    searchInputRef.current?.focus()
                                }}
                            >
                                <XIcon
                                    aria-hidden="true"
                                    className="size-3.5"
                                />
                            </InputGroupButton>
                        ) : !loading ? (
                            <Kbd
                                aria-hidden="true"
                                className="mr-1 hidden bg-background/80 text-muted-foreground/70 lg:inline-flex"
                            >
                                /
                            </Kbd>
                        ) : null}
                        {hasPendingChanges || loading ? (
                            <InputGroupButton
                                id={`${idPrefix}-query`}
                                type="submit"
                                loading={loading}
                                variant={
                                    hasPendingChanges ? "default" : "ghost"
                                }
                                size="xs"
                                aria-label="查询分类"
                                className="mr-0.5 px-2 text-xs font-medium"
                            >
                                搜索
                            </InputGroupButton>
                        ) : (
                            <button
                                id={`${idPrefix}-query`}
                                type="submit"
                                tabIndex={-1}
                                aria-label="查询分类"
                                className="sr-only"
                            />
                        )}
                    </InputGroupAddon>
                </InputGroup>
            </form>
            {filterActive || hasPendingChanges || loading ? (
                <div
                    className="flex items-center justify-between gap-2 px-0.5 text-xs text-muted-foreground"
                    role="status"
                >
                    <span className="truncate">
                        {loading ? (
                            "正在查询…"
                        ) : hasPendingChanges ? (
                            "按回车或点搜索应用条件"
                        ) : (
                            <>
                                命中{" "}
                                <span className="num font-medium text-foreground">
                                    {matchedCount}
                                </span>{" "}
                                个分类
                            </>
                        )}
                    </span>
                    {filterActive ? (
                        <Button
                            id={`${idPrefix}-reset`}
                            type="button"
                            variant="ghost"
                            size="xs"
                            className="h-auto shrink-0 px-1 py-0 text-xs text-primary hover:bg-transparent hover:text-primary/80"
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
