"use client"

import * as React from "react"
import type { RowSelectionState } from "@tanstack/react-table"

import type { MasterDataListItem } from "@/features/master-data/types"

/**
 * 公司商品池勾选：服务端分页下翻页与排序保留已勾选商品，筛选条件变化时清空。
 * 全选只作用于当前已显示的商品。
 *
 * @param rows 当前已显示的商品（表格当前页或卡片已加载部分）
 * @param resetKey 筛选条件标识；变化时清空勾选
 */
export function useSellableGallerySelection(
    rows: readonly MasterDataListItem[],
    resetKey: string,
) {
    const [selected, setSelected] = React.useState<
        ReadonlyMap<string, MasterDataListItem>
    >(() => new Map())

    React.useEffect(() => {
        setSelected((current) => (current.size === 0 ? current : new Map()))
    }, [resetKey])

    const selectedIds = React.useMemo<ReadonlySet<string>>(
        () => new Set(selected.keys()),
        [selected],
    )
    const selectedRows = React.useMemo(() => [...selected.values()], [selected])
    const rowSelection = React.useMemo<RowSelectionState>(
        () =>
            Object.fromEntries(
                rows
                    .filter((row) => selected.has(row.stableId))
                    .map((row) => [row.stableId, true]),
            ),
        [rows, selected],
    )
    /** 表格只回传当前页的勾选状态；其他页的勾选保持不变。 */
    const onRowSelectionChange = React.useCallback(
        (next: RowSelectionState) => {
            setSelected((current) => {
                const updated = new Map(current)
                for (const row of rows) {
                    if (next[row.stableId]) updated.set(row.stableId, row)
                    else updated.delete(row.stableId)
                }
                return updated
            })
        },
        [rows],
    )
    const selectedCount = selected.size
    const shownSelected = rows.filter((row) =>
        selected.has(row.stableId),
    ).length
    const allSelected = rows.length > 0 && shownSelected === rows.length
    const someSelected = selectedCount > 0 && !allSelected

    const toggle = React.useCallback(
        (id: string, checked: boolean) => {
            const row = rows.find((item) => item.stableId === id)
            setSelected((current) => {
                const updated = new Map(current)
                if (checked && row) updated.set(id, row)
                else updated.delete(id)
                return updated
            })
        },
        [rows],
    )

    const selectAllResults = React.useCallback(() => {
        setSelected((current) => {
            const updated = new Map(current)
            for (const row of rows) updated.set(row.stableId, row)
            return updated
        })
    }, [rows])

    const clear = React.useCallback(() => {
        setSelected(new Map())
    }, [])

    return {
        selectedIds,
        selectedRows,
        rowSelection,
        onRowSelectionChange,
        selectedCount,
        allSelected,
        someSelected,
        toggle,
        selectAllResults,
        clear,
    }
}
