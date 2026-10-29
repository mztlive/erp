"use client"

import * as React from "react"
import type { RowSelectionState } from "@tanstack/react-table"
import { useRouter } from "@/lib/navigation"
import type { BatchSelectionView, CreateBatchRequest } from "../api/batch"
import type { CatalogFilters, CatalogItem } from "../api/catalog"
import {
    useCreateAdoptionBatchMutation,
    useSaveAdoptionBatchMutation,
    useBatchSelectionQuery,
    useBatchPreparationQuery,
} from "./batch"

import { newIdempotencyKey } from "../lib/operations"

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
    const [frozenSeed, setFrozen] = React.useState<BatchSelectionView | null>(
        null,
    )
    const [showExcluded, setShowExcluded] = React.useState(false)
    const generation = React.useRef(0)
    const pendingCreate = React.useRef<{ payload: string; key: string } | null>(
        null,
    )
    const identity = { connectionId, batchId: frozenSeed?.batch.id ?? "" }
    const preparationQuery = useBatchPreparationQuery(identity)
    const batch = preparationQuery.data ?? frozenSeed?.batch
    const preparing = Boolean(batch && !batch.selection_complete)
    const batchQuery = useBatchSelectionQuery(
        identity,
        Boolean(batch?.selection_complete),
    )
    const frozen = frozenSeed
        ? (batchQuery.data ?? {
              ...frozenSeed,
              batch: batch ?? frozenSeed.batch,
          })
        : null
    const busy = create.isPending || save.isPending || batchQuery.isFetching
    const needsReload = Boolean(
        frozen && (save.error || batchQuery.error || preparationQuery.error),
    )
    const createSelection = async (
        request: Omit<CreateBatchRequest, "idempotency_key">,
    ) => {
        const payload = JSON.stringify(request)
        const storageKey = `supplier-catalog-selection:${connectionId}`
        let pending = pendingCreate.current
        if (!pending) {
            try {
                pending = JSON.parse(
                    window.sessionStorage.getItem(storageKey) ?? "null",
                )
            } catch {
                /* 使用本次内存中的原操作号。 */
            }
        }
        if (pending?.payload !== payload || !pending.key)
            pending = { payload, key: newIdempotencyKey("selection") }
        pendingCreate.current = pending
        try {
            window.sessionStorage.setItem(storageKey, JSON.stringify(pending))
        } catch {
            /* 当前页面仍保留原操作号用于重试。 */
        }
        const result = await create.mutateAsync({
            connectionId,
            request: { ...request, idempotency_key: pending.key },
        })
        pendingCreate.current = null
        try {
            window.sessionStorage.removeItem(storageKey)
        } catch {
            /* 已受理批次可从历史入口恢复。 */
        }
        return result
    }
    const excludedIds = new Set(frozen?.batch.excluded_ids ?? [])
    const selectedIds = frozen
        ? frozen.rows
              .filter((row) => !excludedIds.has(row.source.id))
              .map((row) => row.source.id)
        : Object.keys(explicit)
    const rowSelection: RowSelectionState = Object.fromEntries(
        selectedIds.map((id) => [id, true]),
    )
    const productCount = frozen
        ? frozen.product_count
        : new Set(Object.values(explicit).map((row) => row.product_id)).size
    const excludedRows =
        frozen?.rows.filter((row) => excludedIds.has(row.source.id)) ?? []
    const clear = () => {
        generation.current += 1
        setExplicit({})
        setFrozen(null)
        setShowExcluded(false)
        create.reset()
        save.reset()
    }
    const saveExclusions = async (excludedIds: string[]) => {
        if (!frozen || busy || needsReload || preparing) return
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
            await batchQuery.refetch()
        } catch {
            // 保存失败时继续展示最后已保存的选择，保留失败原因和重试操作。
        }
    }
    const change = (next: RowSelectionState, items: CatalogItem[]) => {
        if (busy || needsReload || preparing) return
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
            const current = generation.current
            const result = await createSelection({ filter: filters })
            if (current !== generation.current) return
            setFrozen(result)
            setExplicit({})
        } catch {
            // 保留跨页人工选择与筛选。
        }
    }
    const configure = async (itemIds?: string[]) => {
        const ids = itemIds ?? selectedIds
        if (busy || needsReload || preparing || !ids.length) return
        try {
            const result =
                frozen && !itemIds
                    ? frozen
                    : await createSelection({ item_ids: ids })
            router.push(
                `/supplier-api/catalog/adoptions/${encodeURIComponent(result.batch.id)}?connectionId=${encodeURIComponent(connectionId)}`,
            )
        } catch {
            // 读取与保存失败由 mutation 提供具体原因。
        }
    }
    const refreshFrozen = async () => {
        const preparation = await preparationQuery.refetch()
        if (preparation.isError || !preparation.data?.selection_complete) return
        const result = await batchQuery.refetch()
        if (result.data && !result.isError) {
            setFrozen(result.data)
            save.reset()
        }
    }
    return {
        frozen,
        preparing,
        busy,
        needsReload,
        creating: create.isPending,
        selecting:
            create.isPending && Boolean(create.variables?.request.filter),
        configuring: create.isPending && !create.variables?.request.filter,
        saving: save.isPending,
        error:
            save.error ??
            create.error ??
            preparationQuery.error ??
            batchQuery.error,
        rowSelection,
        selectedIds,
        productCount,
        excludedRows,
        showExcluded,
        setShowExcluded,
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
        isExcluded: (itemId: string) => excludedIds.has(itemId),
    }
}
