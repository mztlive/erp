"use client"

import * as React from "react"

import { MultiOptionCombobox } from "@/components/business/multi-option-combobox"
import { OptionCombobox } from "@/components/business/option-combobox"
import { personDirectoryLabel } from "@/features/entity-selectors/api/person-directory"
import {
    usePersonDirectoryList,
    usePersonDirectorySelected,
    type PersonDirectoryFilters,
} from "@/features/entity-selectors/hooks/person-directory"
import { useSearchInput } from "@/features/entity-selectors/hooks/use-search-input"
import { isDataScopeChanged } from "@/features/data-scope/cache"
import { isApiError } from "@/lib/api/errors"

type PersonDirectoryFilterProps = {
    id: string
    category: PersonDirectoryFilters["category"]
    value: string
    onChange: (value: string) => void
    label?: string
    hideLabel?: boolean
    selectionMode?: "single" | "multiple"
    /** 仅当用户在控件上选择了组织筛选时传入。 */
    orgUnitIds?: readonly string[]
    includeDescendants?: boolean
}

function selectedIds(value: string): string[] {
    return [
        ...new Set(
            value
                .split(",")
                .map((id) => id.trim())
                .filter(Boolean),
        ),
    ].sort()
}

function listFailureCopy(error: unknown): string {
    if (isDataScopeChanged(error)) return "人员范围已变化，请重新查询"
    if (isApiError(error) && (error.status === 403 || error.kind === "Auth")) {
        return "没有人员目录权限"
    }
    return "人员候选加载失败，请重试"
}

function selectedFailureCopy(error: unknown): string {
    if (isDataScopeChanged(error)) return "已选人员范围已变化，请重新核对"
    if (isApiError(error) && (error.status === 403 || error.kind === "Auth")) {
        return "没有人员目录权限，已选条件仍保留"
    }
    return "已选人员暂时无法核对，请重试"
}

/** 远程多选人员目录。候选查询不读取业务列表的页码、状态或关键词。 */
export function PersonDirectoryFilter({
    id,
    category,
    value,
    onChange,
    label = "负责人",
    hideLabel = false,
    selectionMode = "multiple",
    orgUnitIds = [],
    includeDescendants = false,
}: PersonDirectoryFilterProps) {
    const search = useSearchInput()
    const ids = selectedIds(value)
    const [pageCount, setPageCount] = React.useState(1)
    const [pageKey, setPageKey] = React.useState("")
    const filterKey = JSON.stringify({
        category,
        q: search.input,
        orgUnitIds: [...orgUnitIds].sort(),
        includeDescendants: orgUnitIds.length > 0 && includeDescendants,
    })
    const effectivePageCount = pageKey === filterKey ? pageCount : 1
    const list = usePersonDirectoryList({
        category,
        q: search.input,
        orgUnitIds,
        includeDescendants,
        pageCount: effectivePageCount,
    })
    const selected = usePersonDirectorySelected(category, ids)
    const pages =
        list.isError || list.isFetching ? [] : (list.data?.pages ?? [])
    const names = new Map<string, string>()
    if (selected.isSuccess && !selected.isFetching && selected.data) {
        for (const item of selected.data.items) {
            names.set(item.id, personDirectoryLabel(item))
        }
    }
    const options = [
        ...pages.flatMap((page) =>
            page.items.filter((item) => !ids.includes(item.id)).map((item) => ({
                value: item.id,
                label: personDirectoryLabel(item),
            })),
        ),
        ...ids.map((id) => ({
                value: id,
                label:
                    names.get(id) ??
                    (selected.isSuccess && !selected.isFetching
                        ? "已选人员（当前不可用）"
                        : "已选人员"),
            })),
    ]
    const seen = new Set<string>()
    const uniqueOptions = options.filter((option) => {
        if (seen.has(option.value)) return false
        seen.add(option.value)
        return true
    })
    const first = pages[0]
    const loaded = pages.reduce((count, page) => count + page.items.length, 0)
    const hasMore = Boolean(first && loaded < first.total && !list.isError)
    const emptyLabel = list.isError
        ? listFailureCopy(list.error)
        : first?.empty_reason === "no_scope"
          ? "当前没有可查询的人员范围"
          : list.isFetching
            ? "正在加载人员"
            : "没有符合条件的人员"

    return (
        <div className={hideLabel ? "min-w-0" : "min-w-0 space-y-1.5"}>
            <label
                className={
                    hideLabel ? "sr-only" : "text-xs text-muted-foreground"
                }
                htmlFor={id}
            >
                {label}
            </label>
            {selectionMode === "single" ? (
                <>
                    <OptionCombobox
                        id={id}
                        aria-label={label}
                        value={ids[0] ?? null}
                        options={uniqueOptions}
                        filterMode="remote"
                        onSearchChange={search.onSearchChange}
                        loading={list.isFetching || selected.isFetching}
                        emptyLabel={emptyLabel}
                        onValueChange={(next) => onChange(next ?? "")}
                        placeholder={`全部${label}`}
                    />
                    {hasMore && (
                        <button
                            id={`${id}-more`}
                            type="button"
                            className="text-xs underline"
                            onClick={() => {
                                setPageKey(filterKey)
                                setPageCount(effectivePageCount + 1)
                            }}
                        >
                            加载更多
                        </button>
                    )}
                </>
            ) : <MultiOptionCombobox
                id={id}
                aria-label={label}
                filterLabel={hideLabel ? label : undefined}
                value={ids}
                options={uniqueOptions}
                filterMode="remote"
                onSearchChange={search.onSearchChange}
                loading={list.isFetching}
                hasMore={hasMore}
                onLoadMore={() => {
                    setPageKey(filterKey)
                    setPageCount(effectivePageCount + 1)
                }}
                loadMoreId={`${id}-more`}
                emptyLabel={emptyLabel}
                onValueChange={(next) =>
                    onChange([...new Set(next)].sort().join(","))
                }
                placeholder={`全部${label}`}
            />}
            {list.isError ? (
                <p className="text-xs text-destructive" role="alert">
                    {listFailureCopy(list.error)}
                    <button
                        id={`${id}-retry`}
                        type="button"
                        className="ml-2 underline"
                        onClick={() => void list.refetch()}
                    >
                        重试
                    </button>
                </p>
            ) : null}
            {selected.isError ? (
                <p className="text-xs text-destructive" role="alert">
                    {selectedFailureCopy(selected.error)}
                    <button
                        id={`${id}-selected-retry`}
                        type="button"
                        className="ml-2 underline"
                        onClick={() => void selected.refetch()}
                    >
                        重试
                    </button>
                </p>
            ) : null}
        </div>
    )
}
