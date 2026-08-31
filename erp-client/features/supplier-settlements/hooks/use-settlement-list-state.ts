"use client"

import * as React from "react"
import type { PaginationState } from "@tanstack/react-table"

import type { SettlementsUrlState } from "@/features/supplier-settlements/lib/url-state"
import {
    hasAppliedSettlementFilters,
    hasStructuredSettlementFilters,
    joinSettlementStatusParam,
    parseSettlementStatusParam,
    validateSettlementPeriodRange,
    type SettlementFilterKey,
} from "@/features/supplier-settlements/lib/settlement-list-filters"
import type { DifferenceType } from "@/features/supplier-settlements/types"

/**
 * 结算列表分页与筛选状态：三层状态模型（docs/ui-filter-design.md §5）。
 * Applied 在 URL（唯一事实源）；Draft 本地受控、不触发请求；面板/错误为 UI 态。
 */
export function useSettlementListState(
    urlState: SettlementsUrlState,
    patchUrl: (patch: Partial<SettlementsUrlState>) => void,
    searchInputRef: React.RefObject<HTMLInputElement | null>,
) {
    // pageSize 固定 50 不入 URL；财务列表不强制加排序（服务端无排序参数），记录在案。
    const pagination = React.useMemo<PaginationState>(
        () => ({
            pageIndex: Math.max(0, urlState.page - 1),
            pageSize: 50,
        }),
        [urlState.page],
    )

    const hasActiveFilters = hasAppliedSettlementFilters(urlState)

    // ---- Draft（本地受控，变化不请求） ----
    const [searchDraft, setSearchDraft] = React.useState(urlState.q ?? "")
    const [supplierIdDraft, setSupplierIdDraft] = React.useState<string | null>(
        urlState.supplierId ?? null,
    )
    const [statusDraft, setStatusDraft] = React.useState<string[]>(() =>
        parseSettlementStatusParam(urlState.status),
    )
    const [differenceTypeDraft, setDifferenceTypeDraft] = React.useState<
        DifferenceType | "all"
    >(urlState.differenceType ?? "all")
    const [periodFromDraft, setPeriodFromDraft] = React.useState(
        urlState.periodFrom ?? "",
    )
    const [periodToDraft, setPeriodToDraft] = React.useState(
        urlState.periodTo ?? "",
    )

    // ---- UI 态 ----
    // 深链带结构化条件时展开面板；展开态本身不写 URL。
    const [panelOpen, setPanelOpen] = React.useState(() =>
        hasStructuredSettlementFilters(urlState),
    )
    const [periodError, setPeriodError] = React.useState<string | null>(null)

    /** 唯一提交路径：收起态 Enter / 尾部箭头与展开态「应用全部筛选」共用。 */
    const applyFilters = React.useCallback(() => {
        const error = validateSettlementPeriodRange(
            periodFromDraft,
            periodToDraft,
        )
        setPeriodError(error)
        if (error) return
        patchUrl({
            q: searchDraft.trim() || undefined,
            supplierId: supplierIdDraft ?? undefined,
            status: joinSettlementStatusParam(statusDraft) ?? undefined,
            differenceType:
                differenceTypeDraft === "all" ? undefined : differenceTypeDraft,
            periodFrom: periodFromDraft.trim() || undefined,
            periodTo: periodToDraft.trim() || undefined,
            page: 1,
        })
        setPanelOpen(false)
    }, [
        differenceTypeDraft,
        patchUrl,
        periodFromDraft,
        periodToDraft,
        searchDraft,
        statusDraft,
        supplierIdDraft,
    ])

    /** 单个 chip 只移除自己的条件；期间上下界作为一个 chip 一起移除。 */
    const removeFilter = React.useCallback(
        (key: SettlementFilterKey) => {
            if (key === "q") {
                setSearchDraft("")
                patchUrl({ q: undefined, page: 1 })
                return
            }
            if (key === "supplierId") {
                setSupplierIdDraft(null)
                patchUrl({ supplierId: undefined, page: 1 })
                return
            }
            if (key === "status") {
                setStatusDraft([])
                patchUrl({ status: undefined, page: 1 })
                return
            }
            if (key === "differenceType") {
                setDifferenceTypeDraft("all")
                patchUrl({ differenceType: undefined, page: 1 })
                return
            }
            setPeriodFromDraft("")
            setPeriodToDraft("")
            setPeriodError(null)
            patchUrl({ periodFrom: undefined, periodTo: undefined, page: 1 })
        },
        [patchUrl],
    )

    /** 只清「更多筛选」结构化条件；保留关键词与视图，面板保持展开。 */
    const resetMoreFilters = React.useCallback(() => {
        setSupplierIdDraft(null)
        setStatusDraft([])
        setDifferenceTypeDraft("all")
        setPeriodFromDraft("")
        setPeriodToDraft("")
        setPeriodError(null)
        patchUrl({
            supplierId: undefined,
            status: undefined,
            differenceType: undefined,
            periodFrom: undefined,
            periodTo: undefined,
            page: 1,
        })
    }, [patchUrl])

    /**
     * 清空全部：重置 Draft、错误、面板、筛选参数与分页；
     * 保留排序、视图、期间与导航上下文（docs/ui-filter-design.md §5.6 / §14.3）。
     */
    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setSupplierIdDraft(null)
        setStatusDraft([])
        setDifferenceTypeDraft("all")
        setPeriodError(null)
        setPanelOpen(false)
        patchUrl({
            q: undefined,
            supplierId: undefined,
            status: undefined,
            differenceType: undefined,
            page: 1,
        })
    }, [patchUrl])

    // ---- URL 回填：只同步 Draft，不抢夺面板展开态 ----
    React.useEffect(() => {
        // 关键词正在编辑时做焦点保护；clearAllFilters 已直接清空草稿，不依赖此 effect。
        if (document.activeElement !== searchInputRef.current) {
            setSearchDraft(urlState.q ?? "")
        }
        setSupplierIdDraft(urlState.supplierId ?? null)
        setStatusDraft(parseSettlementStatusParam(urlState.status))
        setDifferenceTypeDraft(urlState.differenceType ?? "all")
        setPeriodFromDraft(urlState.periodFrom ?? "")
        setPeriodToDraft(urlState.periodTo ?? "")
        setPeriodError(null)
    }, [
        searchInputRef,
        urlState.differenceType,
        urlState.periodFrom,
        urlState.periodTo,
        urlState.q,
        urlState.status,
        urlState.supplierId,
    ])

    return {
        pagination,
        hasActiveFilters,
        searchDraft,
        setSearchDraft,
        panelOpen,
        setPanelOpen,
        supplierIdDraft,
        setSupplierIdDraft,
        statusDraft,
        setStatusDraft,
        differenceTypeDraft,
        setDifferenceTypeDraft,
        periodFromDraft,
        setPeriodFromDraft,
        periodToDraft,
        setPeriodToDraft,
        periodError,
        setPeriodError,
        applyFilters,
        removeFilter,
        resetMoreFilters,
        clearAllFilters,
    }
}
