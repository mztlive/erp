"use client"

import * as React from "react"

import { useAccessUrlState } from "@/features/access-audit/pages/hooks/use-access-url-state"
import {
    auditDateRangeError,
    parseActionFilter,
    parseResultFilter,
    type AccessActionFilterValue,
    type AccessResultFilterValue,
} from "@/features/access-audit/lib/filter-options"
import type { AccessView } from "@/features/access-audit/types"

/** 可被单独移除的已生效条件。 */
export type AccessFilterKey =
    | "q"
    | "time"
    | "action"
    | "result"
    | "actorId"
    | "traceId"
    | "objectId"

/** 用户正在编辑、尚未提交的筛选草稿；"all" 表示该字段的「全部」占位。 */
export type AccessFilterDraft = {
    q: string
    from: string
    to: string
    action: AccessActionFilterValue | "all"
    result: AccessResultFilterValue | "all"
    actorId: string
    traceId: string
    objectId: string
}

export const DEFAULT_ACCESS_FILTER_DRAFT: AccessFilterDraft = {
    q: "",
    from: "",
    to: "",
    action: "all",
    result: "all",
    actorId: "",
    traceId: "",
    objectId: "",
}

/** 已生效筛选（URL 是唯一事实源）；非法枚举在解析时已降级为缺省。 */
export type AccessAppliedFilters = {
    q: string
    from?: string
    to?: string
    action?: AccessActionFilterValue
    result?: AccessResultFilterValue
    actorId?: string
    traceId?: string
    objectId?: string
}

type AccessListFiltersInput = {
    view: AccessView
    /** 筛选写 URL 的统一出口：replace + scroll:false + 回第 1 页。 */
    patchFilterUrl: (patch: Record<string, string | null | undefined>) => void
    searchInputRef: React.RefObject<HTMLInputElement | null>
}

function toDraft(applied: AccessAppliedFilters): AccessFilterDraft {
    return {
        q: applied.q,
        from: applied.from ?? "",
        to: applied.to ?? "",
        action: applied.action ?? "all",
        result: applied.result ?? "all",
        actorId: applied.actorId ?? "",
        traceId: applied.traceId ?? "",
        objectId: applied.objectId ?? "",
    }
}

/** 面板结构化条件是否已生效（不含关键词）。角色 / 用户授权侧没有结构化条件。 */
function hasStructuredFilters(
    applied: AccessAppliedFilters,
    view: AccessView,
): boolean {
    if (view !== "audit") return false
    return Boolean(
        applied.from ||
        applied.to ||
        applied.action ||
        applied.result ||
        applied.actorId ||
        applied.traceId ||
        applied.objectId,
    )
}

/**
 * W19 筛选状态：Applied（URL）/ Draft（本地受控）/ UI（面板与校验）。
 * Draft 变化不写 URL、不触发请求；提交、清除与 chip 移除都走 patchFilterUrl。
 */
export function useAccessListFilters({
    view,
    patchFilterUrl,
    searchInputRef,
}: AccessListFiltersInput) {
    const { searchParams } = useAccessUrlState()

    const applied = React.useMemo<AccessAppliedFilters>(() => {
        const q = searchParams.get("q") ?? ""
        const from = searchParams.get("from")?.trim() || undefined
        const to = searchParams.get("to")?.trim() || undefined
        const actionRaw = parseActionFilter(searchParams.get("action"))
        const resultRaw = parseResultFilter(searchParams.get("result"))
        const actorId = searchParams.get("actorId")?.trim() || undefined
        const traceId = searchParams.get("traceId")?.trim() || undefined
        const objectId = searchParams.get("objectId")?.trim() || undefined
        return {
            q,
            from,
            to,
            action: actionRaw === "all" ? undefined : actionRaw,
            result: resultRaw === "all" ? undefined : resultRaw,
            actorId,
            traceId,
            objectId,
        }
    }, [searchParams])

    const [draft, setDraft] = React.useState<AccessFilterDraft>(() =>
        toDraft(applied),
    )
    const [panelOpen, setPanelOpen] = React.useState(() =>
        hasStructuredFilters(applied, view),
    )
    const [filterError, setFilterError] = React.useState<string | null>(null)

    const updateDraft = React.useCallback(
        <Key extends keyof AccessFilterDraft>(
            key: Key,
            value: AccessFilterDraft[Key],
        ) => {
            setDraft((current) => ({ ...current, [key]: value }))
            if (key === "from" || key === "to") setFilterError(null)
        },
        [],
    )

    const setSearchDraft = React.useCallback((value: string) => {
        setDraft((current) => ({ ...current, q: value }))
    }, [])

    /** 收起态 Enter / 搜索框尾部提交箭头 / 展开态「应用全部筛选」共用同一提交。 */
    const applyFilters = React.useCallback(() => {
        const from = draft.from.trim()
        const to = draft.to.trim()
        const error = auditDateRangeError(from, to)
        setFilterError(error)
        if (error) {
            // 校验失败：不写 URL、不收起面板，展开以展示错误（§5.5）
            setPanelOpen(true)
            return
        }
        patchFilterUrl({
            q: draft.q.trim() || null,
            from: from || null,
            to: to || null,
            action: draft.action === "all" ? null : draft.action,
            result: draft.result === "all" ? null : draft.result,
            actorId: draft.actorId.trim() || null,
            traceId: draft.traceId.trim() || null,
            objectId: draft.objectId.trim() || null,
            page: null,
        })
        setPanelOpen(false)
    }, [draft, patchFilterUrl])

    /** 只清除「更多筛选」结构化条件；保留关键词，面板保持展开。 */
    const resetMoreFilters = React.useCallback(() => {
        setDraft((current) => ({
            ...current,
            from: "",
            to: "",
            action: "all",
            result: "all",
            actorId: "",
            traceId: "",
            objectId: "",
        }))
        setFilterError(null)
        patchFilterUrl({
            from: null,
            to: null,
            action: null,
            result: null,
            actorId: null,
            traceId: null,
            objectId: null,
            page: null,
        })
    }, [patchFilterUrl])

    /** 清空全部：草稿、错误、面板与全部筛选参数同时重置；保留视图与详情/导航上下文。 */
    const clearAllFilters = React.useCallback(() => {
        setDraft(DEFAULT_ACCESS_FILTER_DRAFT)
        setFilterError(null)
        setPanelOpen(false)
        patchFilterUrl({
            q: null,
            from: null,
            to: null,
            action: null,
            result: null,
            actorId: null,
            traceId: null,
            objectId: null,
            page: null,
        })
    }, [patchFilterUrl])

    /** 移除单个已生效条件；时间范围按区间整体移除。 */
    const removeFilter = React.useCallback(
        (key: AccessFilterKey) => {
            switch (key) {
                case "q":
                    setDraft((current) => ({ ...current, q: "" }))
                    break
                case "time":
                    setDraft((current) => ({ ...current, from: "", to: "" }))
                    setFilterError(null)
                    break
                case "action":
                    setDraft((current) => ({ ...current, action: "all" }))
                    break
                case "result":
                    setDraft((current) => ({ ...current, result: "all" }))
                    break
                case "actorId":
                    setDraft((current) => ({ ...current, actorId: "" }))
                    break
                case "traceId":
                    setDraft((current) => ({ ...current, traceId: "" }))
                    break
                case "objectId":
                    setDraft((current) => ({ ...current, objectId: "" }))
                    break
            }
            patchFilterUrl(
                key === "time"
                    ? { from: null, to: null, page: null }
                    : { [key]: null, page: null },
            )
        },
        [patchFilterUrl],
    )

    // 关键词草稿回填：输入框聚焦时保护尚未提交的关键词
    React.useEffect(() => {
        if (document.activeElement !== searchInputRef.current) {
            setDraft((current) => ({ ...current, q: applied.q }))
        }
    }, [applied.q, searchInputRef])

    // 面板草稿回填：只同步已生效筛选，不重置面板展开态
    const appliedSignature = React.useMemo(
        () =>
            [
                applied.from ?? "",
                applied.to ?? "",
                applied.action ?? "",
                applied.result ?? "",
                applied.actorId ?? "",
                applied.traceId ?? "",
                applied.objectId ?? "",
            ].join("\u0000"),
        [applied],
    )
    React.useEffect(() => {
        setDraft((current) => ({ ...toDraft(applied), q: current.q }))
        setFilterError(null)
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [appliedSignature])

    return {
        applied,
        draft,
        updateDraft,
        searchDraft: draft.q,
        setSearchDraft,
        panelOpen,
        setPanelOpen,
        hasStructuredFilters: hasStructuredFilters(applied, view),
        applyFilters,
        resetMoreFilters,
        clearAllFilters,
        removeFilter,
        filterError,
    }
}
