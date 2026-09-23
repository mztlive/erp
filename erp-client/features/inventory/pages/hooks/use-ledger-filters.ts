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
    | "operatorUserIds"
    | "applicantUserIds"
    | "handlerUserIds"

export type LedgerAppliedChip = Readonly<{
    key: LedgerFilterKey
    label: string
}>

export interface UseLedgerFiltersInput {
    view: InventoryView
    /** Applied：URL 是唯一事实源；非法枚举已在解析时降级为默认。 */
    q: string
    warehouseId: string | undefined
    availability: InventoryAvailability
    movementType: string[]
    occurredFrom: string | undefined
    occurredTo: string | undefined
    operatorUserIds: string | undefined
    applicantUserIds: string | undefined
    handlerUserIds: string | undefined
    /** 关键词草稿由 useLedgerSearch 持有（含 `/` 快捷键与回填保护）。 */
    searchDraft: string
    setSearchDraft: React.Dispatch<React.SetStateAction<string>>
    patchUrl: LedgerPatchUrl
    resetPagination: () => void
}

/**
 * 库存台账结构化筛选状态：Applied（URL）/ Draft（本地受控，提交前不请求）/ UI。
 * 查询按钮、Enter 与展开面板共用 applyFilters；重置更多条件只清草稿。
 */
export function useLedgerFilters({
    view,
    q,
    warehouseId,
    availability,
    movementType,
    occurredFrom,
    occurredTo,
    operatorUserIds,
    applicantUserIds,
    handlerUserIds,
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
    const [operatorUserIdsDraft, setOperatorUserIdsDraft] = React.useState(
        operatorUserIds ?? "",
    )
    const [applicantUserIdsDraft, setApplicantUserIdsDraft] = React.useState(
        applicantUserIds ?? "",
    )
    const [handlerUserIdsDraft, setHandlerUserIdsDraft] = React.useState(
        handlerUserIds ?? "",
    )

    // ---- UI 态 ----
    const hasStructuredFilters = Boolean(
        warehouseId ||
        (view === "balance" && availability !== "all") ||
        (view === "movement" && movementType.length > 0) ||
        (view === "movement" && Boolean(occurredFrom || occurredTo)) ||
        (view === "movement" && Boolean(operatorUserIds)) ||
        (view === "adjustment" &&
            Boolean(operatorUserIds || applicantUserIds || handlerUserIds)),
    )
    // 深链只显示已生效标签，不自动打开面板。
    const [panelOpen, setPanelOpen] = React.useState(false)
    const [filterError, setFilterError] = React.useState<string | null>(null)

    /** 唯一提交路径：查询按钮与 Enter 共用。 */
    const applyFilters = React.useCallback(() => {
        const from = occurredFromDraft.trim()
        const to = occurredToDraft.trim()
        const error = ledgerDateRangeError(from, to)
        setFilterError(error)
        if (error) {
            setPanelOpen(true)
            return
        }
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
                operatorUserIds: operatorUserIdsDraft.trim() || null,
                applicantUserIds: applicantUserIdsDraft.trim() || null,
                handlerUserIds: handlerUserIdsDraft.trim() || null,
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
        operatorUserIdsDraft,
        applicantUserIdsDraft,
        handlerUserIdsDraft,
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
            if (key === "operatorUserIds") setOperatorUserIdsDraft("")
            if (key === "applicantUserIds") setApplicantUserIdsDraft("")
            if (key === "handlerUserIds") setHandlerUserIdsDraft("")
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

    /** 只清除「更多筛选」草稿；保留关键词、常用条件与已生效结果。 */
    const resetMoreFilters = React.useCallback(() => {
        if (view === "movement") {
            setMovementTypeDraft([])
            setOccurredFromDraft("")
            setOccurredToDraft("")
            setOperatorUserIdsDraft("")
        }
        if (view === "adjustment") {
            setOperatorUserIdsDraft("")
            setApplicantUserIdsDraft("")
            setHandlerUserIdsDraft("")
        }
        setFilterError(null)
    }, [view])

    /** 取消、关闭、Esc 和外点只恢复低频草稿，保留搜索与常驻仓库、库存条件。 */
    const cancelMoreFilters = React.useCallback(() => {
        setMovementTypeDraft(movementType)
        setOccurredFromDraft(occurredFrom ?? "")
        setOccurredToDraft(occurredTo ?? "")
        setOperatorUserIdsDraft(operatorUserIds ?? "")
        setApplicantUserIdsDraft(applicantUserIds ?? "")
        setHandlerUserIdsDraft(handlerUserIds ?? "")
        setFilterError(null)
        setPanelOpen(false)
    }, [
        applicantUserIds,
        handlerUserIds,
        movementType,
        occurredFrom,
        occurredTo,
        operatorUserIds,
    ])

    /** 清除全部：草稿、错误、面板、全部筛选参数（含来源锁定）与分页同时重置；保留视图与排序。 */
    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setWarehouseIdDraft(null)
        setAvailabilityDraft("all")
        setMovementTypeDraft([])
        setOccurredFromDraft("")
        setOccurredToDraft("")
        setOperatorUserIdsDraft("")
        setApplicantUserIdsDraft("")
        setHandlerUserIdsDraft("")
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
                operatorUserIds: null,
                applicantUserIds: null,
                handlerUserIds: null,
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
                operatorUserIds ?? "",
                applicantUserIds ?? "",
                handlerUserIds ?? "",
            ].join("\u0000"),
        [
            availability,
            movementType,
            occurredFrom,
            occurredTo,
            operatorUserIds,
            applicantUserIds,
            handlerUserIds,
            warehouseId,
        ],
    )
    React.useEffect(() => {
        setWarehouseIdDraft(warehouseId ?? null)
        setAvailabilityDraft(availability)
        setMovementTypeDraft(movementType)
        setOccurredFromDraft(occurredFrom ?? "")
        setOccurredToDraft(occurredTo ?? "")
        setOperatorUserIdsDraft(operatorUserIds ?? "")
        setApplicantUserIdsDraft(applicantUserIds ?? "")
        setHandlerUserIdsDraft(handlerUserIds ?? "")
        setFilterError(null)
        // eslint-disable-next-line react-hooks/exhaustive-deps -- 以稳定签名驱动回填
    }, [appliedSignature])

    const hasPendingChanges =
        searchDraft.trim() !== q.trim() ||
        (warehouseIdDraft ?? null) !== (warehouseId ?? null) ||
        availabilityDraft !== availability ||
        [...movementTypeDraft].sort().join(",") !==
            [...movementType].sort().join(",") ||
        occurredFromDraft !== (occurredFrom ?? "") ||
        occurredToDraft !== (occurredTo ?? "") ||
        operatorUserIdsDraft !== (operatorUserIds ?? "") ||
        applicantUserIdsDraft !== (applicantUserIds ?? "") ||
        handlerUserIdsDraft !== (handlerUserIds ?? "")

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
        operatorUserIdsDraft,
        setOperatorUserIdsDraft,
        applicantUserIdsDraft,
        setApplicantUserIdsDraft,
        handlerUserIdsDraft,
        setHandlerUserIdsDraft,
        panelOpen,
        setPanelOpen,
        hasStructuredFilters,
        filterError,
        setFilterError,
        applyFilters,
        removeFilter,
        resetMoreFilters,
        cancelMoreFilters,
        clearAllFilters,
        hasPendingChanges,
    }
}
