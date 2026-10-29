"use client"

import * as React from "react"
import type { BatchWorkspaceDraft } from "./use-batch-workspace"

type Mapping = BatchWorkspaceDraft["categoryMappings"][number]
export type CategoryMappingStatus = "all" | "missing" | "mapped"

/** 只在用户刷新查询时重算成员与顺序，编辑期间保留当前行位置。 */
function visibleKeys(
    rows: Mapping[],
    keyword: string,
    status: CategoryMappingStatus,
) {
    const query = keyword.trim().toLocaleLowerCase()
    return rows
        .filter(
            (row) =>
                `${row.sourceName} ${row.sourceId}`
                    .toLocaleLowerCase()
                    .includes(query) &&
                (status === "all" ||
                    (status === "mapped"
                        ? Boolean(row.categoryId)
                        : !row.categoryId)),
        )
        .toSorted(
            (left, right) =>
                Number(Boolean(left.categoryId)) -
                    Number(Boolean(right.categoryId)) ||
                right.count - left.count ||
                left.sourceName.localeCompare(right.sourceName),
        )
        .map((row) => row.sourceId)
}

export function useCategoryMappingList(rows: Mapping[]) {
    const [keyword, setKeyword] = React.useState("")
    const [status, setStatus] = React.useState<CategoryMappingStatus>("all")
    const [keys, setKeys] = React.useState(() => visibleKeys(rows, "", "all"))
    const [selected, setSelected] = React.useState<Set<string>>(new Set())
    const [overwrite, setOverwrite] = React.useState(false)
    const [unified, setUnified] = React.useState<string | null>(null)
    const [notice, setNotice] = React.useState("")
    const indexed = new Map(
        rows.map((row, index) => [row.sourceId, { row, index }]),
    )
    const visible = keys.flatMap((key) => {
        const entry = indexed.get(key)
        return entry ? [entry] : []
    })
    const chosen = visible.filter(({ row }) => selected.has(row.sourceId))
    const mappedCount = chosen.filter(({ row }) => row.categoryId).length
    const applicable = chosen.filter(
        ({ row }) =>
            (overwrite || !row.categoryId) && row.categoryId !== unified,
    )
    const refresh = (nextKeyword = keyword, nextStatus = status) => {
        setKeyword(nextKeyword)
        setStatus(nextStatus)
        setKeys(visibleKeys(rows, nextKeyword, nextStatus))
        setSelected(new Set())
        setOverwrite(false)
        setNotice("")
    }
    return {
        keyword,
        status,
        visible,
        chosen,
        mappedCount,
        applicable,
        selected,
        setSelected,
        overwrite,
        setOverwrite,
        unified,
        setUnified,
        notice,
        setNotice,
        refresh,
    }
}
