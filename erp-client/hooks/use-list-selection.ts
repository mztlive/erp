"use client"

import * as React from "react"
import type { RowSelectionState } from "@tanstack/react-table"

const EMPTY_SELECTION: ReadonlyMap<string, never> = new Map<string, never>()

export type UseListSelectionOptions<T> = {
    /** 当前页渲染的行。 */
    pageRows: readonly T[]
    /** 全量或最新查询行（用于刷新或单行更新后回显最新状态，避免持有旧快照）。 */
    latestRows?: readonly T[]
    /** 返回行稳定 ID；禁止使用数组下标或业务名称。 */
    getRowId: (row: T) => string
    /**
     * 筛选条件或视图切换的指纹（如 JSON.stringify(listQueryInput)）。
     * 指纹变化时即时清空已选，避免误操作不可见记录。
     */
    resetKey: string
}

/**
 * 列表跨页多选状态：
 * - 按稳定 ID 跨页保留勾选，兼容前端切片分页与服务端分页；
 * - 筛选或视图切换（resetKey 变化）时即时清空，防止误操作不可见行；
 * - 优先使用最新列表行对象回显状态，避免单行更新或刷新后持有过期快照。
 */
export function useListSelection<T>({
    pageRows,
    latestRows,
    getRowId,
    resetKey,
}: UseListSelectionOptions<T>) {
    const [state, setState] = React.useState<{
        scope: string
        map: ReadonlyMap<string, T>
    }>({ scope: resetKey, map: EMPTY_SELECTION })

    const activeSelected: ReadonlyMap<string, T> =
        state.scope === resetKey ? state.map : EMPTY_SELECTION

    const latestById = React.useMemo(
        () =>
            new Map(
                (latestRows ?? pageRows).map((row) => [getRowId(row), row]),
            ),
        [getRowId, latestRows, pageRows],
    )

    const selectedRows = React.useMemo<readonly T[]>(
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
                const base: ReadonlyMap<string, T> =
                    current.scope === resetKey ? current.map : EMPTY_SELECTION
                const updated = new Map(base)
                for (const id of updated.keys()) {
                    if (!next[id]) updated.delete(id)
                }
                for (const row of pageRows) {
                    const id = getRowId(row)
                    if (next[id]) {
                        updated.set(id, row)
                    }
                }
                return { scope: resetKey, map: updated }
            })
        },
        [getRowId, pageRows, resetKey],
    )

    const selectAllPage = React.useCallback(() => {
        setState((current) => {
            const base: ReadonlyMap<string, T> =
                current.scope === resetKey ? current.map : EMPTY_SELECTION
            const updated = new Map(base)
            for (const row of pageRows) {
                updated.set(getRowId(row), row)
            }
            return { scope: resetKey, map: updated }
        })
    }, [getRowId, pageRows, resetKey])

    const remove = React.useCallback(
        (rowId: string) => {
            setState((current) => {
                const base: ReadonlyMap<string, T> =
                    current.scope === resetKey ? current.map : EMPTY_SELECTION
                if (!base.has(rowId)) return current
                const updated = new Map(base)
                updated.delete(rowId)
                return { scope: resetKey, map: updated }
            })
        },
        [resetKey],
    )

    const retainOnly = React.useCallback(
        (ids: readonly string[]) => {
            const keep = new Set(ids)
            setState((current) => {
                const base: ReadonlyMap<string, T> =
                    current.scope === resetKey ? current.map : EMPTY_SELECTION
                const updated = new Map<string, T>()
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
        activeSelected.has(getRowId(row)),
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
