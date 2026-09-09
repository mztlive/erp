"use client"

import * as React from "react"

import type { BatchAppliedChip } from "@/features/import-opening/components/batch-list-toolbar"
import { useBatchSearchDraft } from "@/features/import-opening/hooks/use-batch-search"
import type { ImportOpeningUrlState } from "@/features/import-opening/lib/url-state"
import {
    OBJECT_CODE_LABEL,
    type ImportObjectCode,
} from "@/features/import-opening/types"

/** 可被单独移除的已生效批次筛选条件。 */
export type BatchFilterKey = "q" | "objectType"

export type BatchObjectTypeDraft = ImportObjectCode | "all"
const OBJECT_CODE_VALUES = Object.keys(
    OBJECT_CODE_LABEL,
) as readonly ImportObjectCode[]

function sanitizeBatchObjectType(
    value: ImportObjectCode | undefined,
): ImportObjectCode | undefined {
    return OBJECT_CODE_VALUES.includes(value as ImportObjectCode)
        ? (value as ImportObjectCode)
        : undefined
}

/**
 * 批次列表筛选三层状态：
 * Applied 在 URL（唯一事实源）、Draft 本地受控（不触发请求）、UI 态本地。
 * 查询按钮与 Enter 共用 applyBatchFilters。
 */
export function useBatchListFilters({
    urlState,
    patchUrl,
}: {
    urlState: ImportOpeningUrlState
    patchUrl: (patch: Partial<ImportOpeningUrlState>) => void
}) {
    const q = urlState.q ?? ""
    const appliedObjectType = sanitizeBatchObjectType(urlState.objectType)

    const { qDraft, setQDraft, searchInputRef } = useBatchSearchDraft(q)
    const [objectTypeDraft, setObjectTypeDraft] =
        React.useState<BatchObjectTypeDraft>(appliedObjectType ?? "all")
    const hasStructuredBatchFilters = Boolean(appliedObjectType)
    const hasAppliedBatchFilters = Boolean(q.trim() || appliedObjectType)

    /** 单一提交路径：一次性写入全部筛选参数并回第 1 页（§5.3）。 */
    const applyBatchFilters = React.useCallback(() => {
        patchUrl({
            q: qDraft.trim() || undefined,
            objectType: objectTypeDraft === "all" ? undefined : objectTypeDraft,
            status: undefined,
            page: 1,
        })
    }, [objectTypeDraft, patchUrl, qDraft])

    /** 移除单个已生效条件；同步草稿并回第 1 页（§3.6）。 */
    const removeBatchFilter = React.useCallback(
        (key: BatchFilterKey) => {
            const patch: Partial<ImportOpeningUrlState> = { page: 1 }
            if (key === "q") {
                setQDraft("")
                patch.q = undefined
            } else if (key === "objectType") {
                setObjectTypeDraft("all")
                patch.objectType = undefined
            }
            patchUrl(patch)
        },
        [patchUrl, setQDraft],
    )

    /** 全部清除：草稿、URL 筛选参数与分页一起重置（§5.6）。 */
    const clearAllBatchFilters = React.useCallback(() => {
        setQDraft("")
        setObjectTypeDraft("all")
        patchUrl({
            q: undefined,
            objectType: undefined,
            status: undefined,
            page: 1,
        })
    }, [patchUrl, setQDraft])

    // URL 回填：结构化草稿跟随 Applied（§5.4）。
    // 关键词草稿回填由 useBatchSearchDraft 承担（含焦点保护）。
    React.useEffect(() => {
        setObjectTypeDraft(appliedObjectType ?? "all")
    }, [appliedObjectType])

    const appliedChips = React.useMemo<readonly BatchAppliedChip[]>(() => {
        const chips: BatchAppliedChip[] = []
        if (q.trim()) {
            chips.push({ key: "q", label: `搜索：${q.trim()}` })
        }
        if (appliedObjectType) {
            chips.push({
                key: "objectType",
                label: `对象：${OBJECT_CODE_LABEL[appliedObjectType]}`,
            })
        }
        return chips
    }, [appliedObjectType, q])

    const hasPendingChanges =
        qDraft.trim() !== q.trim() ||
        objectTypeDraft !== (appliedObjectType ?? "all")

    return {
        q,
        appliedObjectType,
        hasStructuredBatchFilters,
        hasAppliedBatchFilters,
        qDraft,
        setQDraft,
        objectTypeDraft,
        setObjectTypeDraft,
        searchInputRef,
        appliedChips,
        applyBatchFilters,
        removeBatchFilter,
        clearAllBatchFilters,
        hasPendingChanges,
    }
}
