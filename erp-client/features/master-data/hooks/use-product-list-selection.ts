"use client"

import * as React from "react"
import type { RowSelectionState } from "@tanstack/react-table"

import type { MasterDataListItem } from "@/features/master-data/types"

const EMPTY_SELECTION: ReadonlyMap<string, MasterDataListItem> = new Map()

/**
 * 商品列表多选状态：
 * - 按稳定商品 ID 跨页保留勾选，兼容前端切片分页与后续服务端分页改造；
 * - 筛选或生命周期视图切换（resetKey 变化）时即时清空，防止误操作不可见商品；
 * - 优先使用最新列表行对象回显状态，避免单行开关或刷新后持有过期快照。
 */
export function useProductListSelection({
    pageRows,
    latestRows,
    resetKey,
}: {
    pageRows: readonly MasterDataListItem[]
    latestRows?: readonly MasterDataListItem[]
    resetKey: string
}) {
    const [state, setState] = React.useState<{
        scope: string
        map: ReadonlyMap<string, MasterDataListItem>
    }>({ scope: resetKey, map: EMPTY_SELECTION })

    const activeSelected =
        state.scope === resetKey ? state.map : EMPTY_SELECTION

    const latestById = React.useMemo(
        () =>
            new Map((latestRows ?? pageRows).map((row) => [row.stableId, row])),
        [latestRows, pageRows],
    )

    const selectedRows = React.useMemo<readonly MasterDataListItem[]>(
        () =>
            [...activeSelected.entries()].map(
                ([id, cached]) => latestById.get(id) ?? cached,
            ),
        [activeSelected, latestById],
    )

    const selectedIds = React.useMemo<ReadonlySet<string>>(
        () => new Set(activeSelected.keys()),
        [activeSelected],
    )

    /** 传入全量已选键（仅 true），让表头半选、跨页回显与分页条已选总数保持一致。 */
    const rowSelection = React.useMemo<RowSelectionState>(
        () =>
            Object.fromEntries(
                [...activeSelected.keys()].map((id) => [id, true]),
            ),
        [activeSelected],
    )

    const onRowSelectionChange = React.useCallback(
        (next: RowSelectionState) => {
            setState((current) => {
                const base =
                    current.scope === resetKey ? current.map : EMPTY_SELECTION
                const updated = new Map(base)
                for (const id of updated.keys()) {
                    if (!next[id]) updated.delete(id)
                }
                for (const row of pageRows) {
                    if (next[row.stableId]) {
                        updated.set(row.stableId, row)
                    }
                }
                return { scope: resetKey, map: updated }
            })
        },
        [pageRows, resetKey],
    )

    const selectAllPage = React.useCallback(() => {
        setState((current) => {
            const base =
                current.scope === resetKey ? current.map : EMPTY_SELECTION
            const updated = new Map(base)
            for (const row of pageRows) {
                updated.set(row.stableId, row)
            }
            return { scope: resetKey, map: updated }
        })
    }, [pageRows, resetKey])

    const remove = React.useCallback(
        (productId: string) => {
            setState((current) => {
                const base =
                    current.scope === resetKey ? current.map : EMPTY_SELECTION
                if (!base.has(productId)) return current
                const updated = new Map(base)
                updated.delete(productId)
                return { scope: resetKey, map: updated }
            })
        },
        [resetKey],
    )

    const retainOnly = React.useCallback(
        (ids: readonly string[]) => {
            const keep = new Set(ids)
            setState((current) => {
                const base =
                    current.scope === resetKey ? current.map : EMPTY_SELECTION
                const updated = new Map<string, MasterDataListItem>()
                for (const [id, row] of base.entries()) {
                    if (keep.has(id)) {
                        updated.set(id, latestById.get(id) ?? row)
                    }
                }
                return { scope: resetKey, map: updated }
            })
        },
        [latestById, resetKey],
    )

    const clear = React.useCallback(() => {
        setState({ scope: resetKey, map: EMPTY_SELECTION })
    }, [resetKey])

    const selectedCount = activeSelected.size
    const pageSelectedCount = pageRows.filter((row) =>
        activeSelected.has(row.stableId),
    ).length
    const allPageSelected =
        pageRows.length > 0 && pageSelectedCount === pageRows.length

    return {
        selectedIds,
        selectedRows,
        selectedCount,
        allPageSelected,
        rowSelection,
        onRowSelectionChange,
        selectAllPage,
        remove,
        retainOnly,
        clear,
    }
}
