"use client"

import * as React from "react"

import { ledgerDateRangeError } from "@/features/inventory/lib/presentation"
import type {
    InventoryAvailability,
    InventoryView,
} from "@/features/inventory/types"
import type { LedgerPatchUrl } from "./use-inventory-ledger-url-state"

/** 可被单独移除的已生效条件。 */
export type LedgerFilterKey =
    | "q"
    | "warehouseId"
    | "availability"
    | "movementType"
    | "occurredRange"
    | "skuId"
    | "salesOrderLineId"
    | "adjustmentId"

export type LedgerAppliedChip = Readonly<{
    key: LedgerFilterKey
    label: string
}>

export interface UseLedgerFiltersInput {
    view: InventoryView
    /** Applied：URL 是唯一事实源；非法枚举已在解析时降级为默认。 */
    warehouseId: string | undefined
    availability: InventoryAvailability
    movementType: string[]
    occurredFrom: string | undefined
    occurredTo: string | undefined
    /** 关键词草稿由 useLedgerSearch 持有（含 `/` 快捷键与回填保护）。 */
    searchDraft: string
    setSearchDraft: React.Dispatch<React.SetStateAction<string>>
    patchUrl: LedgerPatchUrl
    resetPagination: () => void
}

/**
 * 库存台账结构化筛选状态（docs/ui-filter-design.md §5 / §8.2）：
 * Applied（URL）/ Draft（本地受控，提交前不请求）/ UI（面板展开与校验）。
 * 收起态 Enter、搜索框尾部提交箭头与展开态「应用全部筛选」共用 applyFilters。
 */
export function useLedgerFilters({
    view,
    warehouseId,
    availability,
    movementType,
    occurredFrom,
    occurredTo,
    searchDraft,
    setSearchDraft,
    patchUrl,
    resetPagination,
}: UseLedgerFiltersInput) {
    // ---- Draft：本地受控，提交前不触发请求 ----
    const [warehouseIdDraft, setWarehouseIdDraft] = React.useState<
        string | null
    >(warehouseId ?? null)
    const [availabilityDraft, setAvailabilityDraft] =
        React.useState<InventoryAvailability>(availability)
    const [movementTypeDraft, setMovementTypeDraft] =
        React.useState<string[]>(movementType)
    const [occurredFromDraft, setOccurredFromDraft] = React.useState(
        occurredFrom ?? "",
    )
    const [occurredToDraft, setOccurredToDraft] = React.useState(
        occurredTo ?? "",
    )

    // ---- UI 态 ----
    const hasStructuredFilters = Boolean(
        warehouseId ||
        (view === "balance" && availability !== "all") ||
        (view === "movement" && movementType.length > 0) ||
        (view === "movement" && Boolean(occurredFrom || occurredTo)),
    )
    // 有结构化条件的初始深链展开面板；URL 回填不得再次强制展开（§5.4 / §5.5）
    const [panelOpen, setPanelOpen] = React.useState(hasStructuredFilters)
    const [filterError, setFilterError] = React.useState<string | null>(null)

    /** 唯一提交路径：收起态 Enter / 尾部箭头 / 展开态「应用全部筛选」共用。 */
    const applyFilters = React.useCallback(() => {
        const from = occurredFromDraft.trim()
        const to = occurredToDraft.trim()
        const error = ledgerDateRangeError(from, to)
        setFilterError(error)
        if (error) return
        patchUrl(
            {
                q: searchDraft.trim() || null,
                warehouseId: warehouseIdDraft,
                availability:
                    availabilityDraft === "all" ? null : availabilityDraft,
                movementType:
                    movementTypeDraft.length > 0
                        ? Array.from(new Set(movementTypeDraft))
                              .sort()
                              .join(",")
                        : null,
                occurredFrom: from || null,
                occurredTo: to || null,
            },
            { replace: true, scroll: false },
        )
        resetPagination()
        setPanelOpen(false)
    }, [
        availabilityDraft,
        movementTypeDraft,
        occurredFromDraft,
        occurredToDraft,
        patchUrl,
        resetPagination,
        searchDraft,
        warehouseIdDraft,
    ])

    /** 移除单个已生效条件；发生日期按区间整体移除。来源锁定参数只存在于 URL。 */
    const removeFilter = React.useCallback(
        (key: LedgerFilterKey) => {
            if (key === "q") setSearchDraft("")
            if (key === "warehouseId") setWarehouseIdDraft(null)
            if (key === "availability") setAvailabilityDraft("all")
            if (key === "movementType") setMovementTypeDraft([])
            if (key === "occurredRange") {
                setOccurredFromDraft("")
                setOccurredToDraft("")
                setFilterError(null)
            }
            patchUrl(
                key === "occurredRange"
                    ? { occurredFrom: null, occurredTo: null }
                    : { [key]: null },
                { replace: true, scroll: false },
            )
            resetPagination()
        },
        [patchUrl, resetPagination, setSearchDraft],
    )

    /** 只清除「更多筛选」结构化条件；保留关键词与来源锁定，面板保持展开。 */
    const resetMoreFilters = React.useCallback(() => {
        setWarehouseIdDraft(null)
        setAvailabilityDraft("all")
        setMovementTypeDraft([])
        setOccurredFromDraft("")
        setOccurredToDraft("")
        setFilterError(null)
        patchUrl(
            {
                warehouseId: null,
                availability: null,
                movementType: null,
                occurredFrom: null,
                occurredTo: null,
            },
            { replace: true, scroll: false },
        )
        resetPagination()
    }, [patchUrl, resetPagination])

    /** 清空全部：草稿、错误、面板、全部筛选参数（含来源锁定）与分页同时重置；保留视图与排序。 */
    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setWarehouseIdDraft(null)
        setAvailabilityDraft("all")
        setMovementTypeDraft([])
        setOccurredFromDraft("")
        setOccurredToDraft("")
        setFilterError(null)
        setPanelOpen(false)
        patchUrl(
            {
                q: null,
                warehouseId: null,
                availability: null,
                movementType: null,
                occurredFrom: null,
                occurredTo: null,
                skuId: null,
                salesOrderLineId: null,
                adjustmentId: null,
            },
            { replace: true, scroll: false },
        )
        resetPagination()
    }, [patchUrl, resetPagination, setSearchDraft])

    // URL 回填只同步 Draft；不重置面板展开态（§5.4 / §5.5）。
    const appliedSignature = React.useMemo(
        () =>
            [
                warehouseId ?? "",
                availability,
                movementType.join(","),
                occurredFrom ?? "",
                occurredTo ?? "",
            ].join("\u0000"),
        [availability, movementType, occurredFrom, occurredTo, warehouseId],
    )
    React.useEffect(() => {
        setWarehouseIdDraft(warehouseId ?? null)
        setAvailabilityDraft(availability)
        setMovementTypeDraft(movementType)
        setOccurredFromDraft(occurredFrom ?? "")
        setOccurredToDraft(occurredTo ?? "")
        setFilterError(null)
        // eslint-disable-next-line react-hooks/exhaustive-deps -- 以稳定签名驱动回填
    }, [appliedSignature])

    return {
        searchDraft,
        setSearchDraft,
        warehouseIdDraft,
        setWarehouseIdDraft,
        availabilityDraft,
        setAvailabilityDraft,
        movementTypeDraft,
        setMovementTypeDraft,
        occurredFromDraft,
        setOccurredFromDraft,
        occurredToDraft,
        setOccurredToDraft,
        panelOpen,
        setPanelOpen,
        hasStructuredFilters,
        filterError,
        setFilterError,
        applyFilters,
        removeFilter,
        resetMoreFilters,
        clearAllFilters,
    }
}
