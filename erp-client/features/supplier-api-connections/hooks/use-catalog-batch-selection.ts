"use client"

import * as React from "react"
import type { RowSelectionState } from "@tanstack/react-table"
import { useRouter } from "@/lib/navigation"
import type { BatchView } from "../api/batch"
import type { CatalogFilters, CatalogItem } from "../api/catalog"
import {
    useCreateAdoptionBatchMutation,
    useSaveAdoptionBatchMutation,
    useAdoptionBatchQuery,
} from "./batch"

export function catalogFilterKey(filters: CatalogFilters) {
    return JSON.stringify({
        keyword: filters.keyword?.trim() || undefined,
        category_ids: [
            ...(filters.category_ids ??
                (filters.category_id ? [filters.category_id] : [])),
        ].sort(),
        brand_ids: [
            ...(filters.brand_ids ??
                (filters.brand_id ? [filters.brand_id] : [])),
        ].sort(),
        include_descendants: Boolean(filters.include_descendants),
        only_unadopted: Boolean(filters.only_unadopted),
        price_min: filters.price_min || undefined,
        price_max: filters.price_max || undefined,
        availability: filters.availability || undefined,
        selection_status:
            filters.selection_status === "ALL"
                ? undefined
                : filters.selection_status,
    })
}

export function useCatalogBatchSelection(connectionId: string) {
    const router = useRouter()
    const create = useCreateAdoptionBatchMutation()
    const save = useSaveAdoptionBatchMutation()
    const [explicit, setExplicit] = React.useState<Record<string, CatalogItem>>(
        {},
    )
    const [frozen, setFrozen] = React.useState<BatchView | null>(null)
    const [notice, setNotice] = React.useState<string | null>(null)
    const [showExcluded, setShowExcluded] = React.useState(false)
    const batchQuery = useAdoptionBatchQuery({
        connectionId,
        batchId: frozen?.batch.id ?? "",
    })
    const busy = create.isPending || save.isPending || batchQuery.isFetching
    const needsReload = Boolean(frozen && (save.error || batchQuery.error))
    const selectedIds = frozen
        ? frozen.rows
              .filter(
                  (row) => !frozen.batch.excluded_ids.includes(row.source.id),
              )
              .map((row) => row.source.id)
        : Object.keys(explicit)
    const rowSelection: RowSelectionState = Object.fromEntries(
        selectedIds.map((id) => [id, true]),
    )
    const productCount = frozen
        ? frozen.product_count
        : new Set(Object.values(explicit).map((row) => row.product_id)).size
    const excludedRows =
        frozen?.rows.filter((row) =>
            frozen.batch.excluded_ids.includes(row.source.id),
        ) ?? []
    const clear = (message?: string) => {
        setExplicit({})
        setFrozen(null)
        setShowExcluded(false)
        setNotice(message ?? null)
        create.reset()
        save.reset()
    }
    const saveExclusions = async (excludedIds: string[]) => {
        if (!frozen || busy || needsReload) return
        try {
            const result = await save.mutateAsync({
                connectionId,
                batchId: frozen.batch.id,
                request: {
                    expected_version: frozen.batch.version,
                    excluded_ids: excludedIds,
                },
            })
            setFrozen(result)
        } catch {
            // 保存失败时继续展示最后已保存的选择，保留失败原因和重试操作。
        }
    }
    const change = (next: RowSelectionState, items: CatalogItem[]) => {
        if (busy || needsReload) return
        setNotice(null)
        if (frozen) {
            void saveExclusions(
                frozen.rows
                    .filter((row) => !next[row.source.id])
                    .map((row) => row.source.id),
            )
            return
        }
        setExplicit((previous) => {
            const updated = Object.fromEntries(
                Object.entries(previous).filter(([id]) => next[id]),
            )
            for (const item of items) if (next[item.id]) updated[item.id] = item
            return updated
        })
    }
    const freezeAll = async (filters: CatalogFilters) => {
        if (busy) return
        try {
            const result = await create.mutateAsync({
                connectionId,
                request: { filter: filters },
            })
            setFrozen(result)
            setExplicit({})
            setNotice("已保存全部筛选结果；之后新同步的商品不会加入本次选择。")
        } catch {
            // 保留跨页人工选择与筛选。
        }
    }
    const configure = async (itemIds?: string[]) => {
        const ids = itemIds ?? selectedIds
        if (busy || needsReload || !ids.length) return
        try {
            const result =
                frozen && !itemIds
                    ? frozen
                    : await create.mutateAsync({
                          connectionId,
                          request: { item_ids: ids },
                      })
            router.push(
                `/supplier-api/catalog/adoptions/${encodeURIComponent(result.batch.id)}?connectionId=${encodeURIComponent(connectionId)}`,
            )
        } catch {
            // 读取与保存失败由 mutation 提供具体原因。
        }
    }
    const refreshFrozen = async () => {
        const result = await batchQuery.refetch()
        if (result.data && !result.isError) {
            setFrozen(result.data)
            save.reset()
            setNotice("已重新读取已保存的选择，请核对后继续。")
        }
    }
    return {
        frozen,
        busy,
        needsReload,
        creating: create.isPending,
        saving: save.isPending,
        error: save.error ?? create.error ?? batchQuery.error,
        rowSelection,
        selectedIds,
        productCount,
        excludedRows,
        showExcluded,
        setShowExcluded,
        notice,
        clear,
        change,
        freezeAll,
        configure,
        refreshFrozen,
        restore: (itemId: string) =>
            saveExclusions(
                frozen?.batch.excluded_ids.filter((id) => id !== itemId) ?? [],
            ),
        isFrozenMember: (itemId: string) =>
            !frozen || frozen.rows.some((row) => row.source.id === itemId),
        isExcluded: (itemId: string) =>
            frozen?.batch.excluded_ids.includes(itemId) ?? false,
    }
}
