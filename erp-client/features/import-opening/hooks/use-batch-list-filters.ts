"use client"

import * as React from "react"

import type { BatchAppliedChip } from "@/features/import-opening/components/batch-list-toolbar"
import { useBatchSearchDraft } from "@/features/import-opening/hooks/use-batch-search"
import type { ImportOpeningUrlState } from "@/features/import-opening/lib/url-state"
import {
    BATCH_STATUS_LABEL,
    OBJECT_CODE_LABEL,
    type ImportBatchStatus,
    type ImportObjectCode,
} from "@/features/import-opening/types"

/** 可被单独移除的已生效批次筛选条件。 */
export type BatchFilterKey = "q" | "objectType" | "status"

export type BatchObjectTypeDraft = ImportObjectCode | "all"
export type BatchStatusDraft = ImportBatchStatus | "all"

const BATCH_STATUS_VALUES = Object.keys(
    BATCH_STATUS_LABEL,
) as readonly ImportBatchStatus[]
const OBJECT_CODE_VALUES = Object.keys(
    OBJECT_CODE_LABEL,
) as readonly ImportObjectCode[]

/** URL 中的非法枚举值在解析时降级为默认值，不继续传给接口（§6.1）。 */
function sanitizeBatchStatus(
    value: string | undefined,
): ImportBatchStatus | undefined {
    return BATCH_STATUS_VALUES.includes(value as ImportBatchStatus)
        ? (value as ImportBatchStatus)
        : undefined
}

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
 * 收起态 Enter / 提交箭头与展开态「应用全部筛选」共用 applyBatchFilters。
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
    const appliedStatus = sanitizeBatchStatus(urlState.status)

    const { qDraft, setQDraft, searchInputRef } = useBatchSearchDraft(q)
    const [objectTypeDraft, setObjectTypeDraft] =
        React.useState<BatchObjectTypeDraft>(appliedObjectType ?? "all")
    const [statusDraft, setStatusDraft] = React.useState<BatchStatusDraft>(
        appliedStatus ?? "all",
    )
    /** 有结构化条件的初始深链展开面板；提交成功后的 URL 回填不重新展开（§5.5）。 */
    const [batchFilterPanelOpen, setBatchFilterPanelOpen] = React.useState(
        Boolean(appliedStatus || appliedObjectType),
    )

    const hasStructuredBatchFilters = Boolean(
        appliedObjectType || appliedStatus,
    )
    const hasAppliedBatchFilters = Boolean(
        q.trim() || appliedObjectType || appliedStatus,
    )

    /** 单一提交路径：一次性写入全部筛选参数并回第 1 页（§5.3）。 */
    const applyBatchFilters = React.useCallback(() => {
        patchUrl({
            q: qDraft.trim() || undefined,
            objectType:
                objectTypeDraft === "all" ? undefined : objectTypeDraft,
            status: statusDraft === "all" ? undefined : statusDraft,
            page: 1,
        })
        setBatchFilterPanelOpen(false)
    }, [objectTypeDraft, patchUrl, qDraft, statusDraft])

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
            } else {
                setStatusDraft("all")
                patch.status = undefined
            }
            patchUrl(patch)
        },
        [patchUrl, setQDraft],
    )

    /** 仅清结构化条件；保留关键词与快捷筛选，保持面板展开（§5.6）。 */
    const resetMoreBatchFilters = React.useCallback(() => {
        setObjectTypeDraft("all")
        setStatusDraft("all")
        patchUrl({ objectType: undefined, status: undefined, page: 1 })
    }, [patchUrl])

    /** 全部清除：草稿、面板、URL 筛选参数与分页一起重置（§5.6）。 */
    const clearAllBatchFilters = React.useCallback(() => {
        setQDraft("")
        setObjectTypeDraft("all")
        setStatusDraft("all")
        setBatchFilterPanelOpen(false)
        patchUrl({
            q: undefined,
            objectType: undefined,
            status: undefined,
            page: 1,
        })
    }, [patchUrl, setQDraft])

    // URL 回填：结构化草稿跟随 Applied（§5.4）；面板展开态不受回填影响。
    // 关键词草稿回填由 useBatchSearchDraft 承担（含焦点保护）。
    React.useEffect(() => {
        setObjectTypeDraft(appliedObjectType ?? "all")
        setStatusDraft(appliedStatus ?? "all")
    }, [appliedObjectType, appliedStatus])

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
        if (appliedStatus) {
            chips.push({
                key: "status",
                label: `状态：${BATCH_STATUS_LABEL[appliedStatus]}`,
            })
        }
        return chips
    }, [appliedObjectType, appliedStatus, q])

    return {
        q,
        appliedObjectType,
        appliedStatus,
        hasStructuredBatchFilters,
        hasAppliedBatchFilters,
        qDraft,
        setQDraft,
        objectTypeDraft,
        setObjectTypeDraft,
        statusDraft,
        setStatusDraft,
        batchFilterPanelOpen,
        setBatchFilterPanelOpen,
        searchInputRef,
        appliedChips,
        applyBatchFilters,
        removeBatchFilter,
        resetMoreBatchFilters,
        clearAllBatchFilters,
    }
}
