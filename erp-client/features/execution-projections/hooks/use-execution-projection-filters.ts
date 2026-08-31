"use client"

import * as React from "react"

import { useExecutionProjectionSearch } from "@/features/execution-projections/hooks/use-execution-projection-search"
import { useExecutionProjectionUrlState } from "@/features/execution-projections/hooks/use-execution-projection-url-state"
import type {
    LatencyBand,
    ProjectionSource,
    ReconciliationStatus,
} from "@/features/execution-projections/types"

/** 可被单独移除的已生效条件（与 URL 参数一一对应）。 */
export type ExecutionProjectionFilterKey =
    | "q"
    | "mall"
    | "deliveryStatus"
    | "latency"
    | "reconciliation"
    | "source"
    | "metric"

export type ExecutionProjectionAppliedChip = Readonly<{
    key: ExecutionProjectionFilterKey
    label: string
}>

/** URL 中是否存在已生效的结构化（「更多筛选」内）条件。 */
export function hasStructuredExecutionProjectionFilters(state: {
    mallId: string
    deliveryStatus: string
    latency: LatencyBand | "all"
    reconciliation: ReconciliationStatus | "all"
    source: ProjectionSource | "all"
}): boolean {
    return (
        state.mallId !== "all" ||
        state.deliveryStatus !== "all" ||
        state.latency !== "all" ||
        state.reconciliation !== "all" ||
        state.source !== "all"
    )
}

/**
 * W23 执行信息筛选状态：Applied 在 URL（唯一事实源），Draft 与面板展开态在本地。
 * 关键词和结构化条件经同一个 `applyFilters` 一次性提交；指标条快捷筛选仍直接写 URL。
 */
export function useExecutionProjectionFilters() {
    const urlState = useExecutionProjectionUrlState()
    const { replaceParams } = urlState
    const { searchDraft, setSearchDraft, searchInputRef } =
        useExecutionProjectionSearch(urlState.q)

    const [mallIdDraft, setMallIdDraft] = React.useState(urlState.mallId)
    const [deliveryStatusDraft, setDeliveryStatusDraft] = React.useState(
        urlState.deliveryStatus,
    )
    const [latencyDraft, setLatencyDraft] = React.useState(urlState.latency)
    const [reconciliationDraft, setReconciliationDraft] = React.useState(
        urlState.reconciliation,
    )
    const [sourceDraft, setSourceDraft] = React.useState(urlState.source)
    const [panelOpen, setPanelOpen] = React.useState(() =>
        hasStructuredExecutionProjectionFilters(urlState),
    )

    /** 收起态 Enter 与展开态「应用全部筛选」共用同一条提交路径。 */
    const applyFilters = React.useCallback(() => {
        replaceParams({
            q: searchDraft.trim() || null,
            mall: mallIdDraft === "all" ? null : mallIdDraft,
            deliveryStatus:
                deliveryStatusDraft === "all" ? null : deliveryStatusDraft,
            latency: latencyDraft === "all" ? null : latencyDraft,
            reconciliation:
                reconciliationDraft === "all" ? null : reconciliationDraft,
            source: sourceDraft === "all" ? null : sourceDraft,
            page: null,
        })
        setPanelOpen(false)
    }, [
        deliveryStatusDraft,
        latencyDraft,
        mallIdDraft,
        reconciliationDraft,
        replaceParams,
        searchDraft,
        sourceDraft,
    ])

    /** 仅清除「更多筛选」结构化条件；保留关键词与指标快捷筛选，面板保持展开。 */
    const resetMoreFilters = React.useCallback(() => {
        setMallIdDraft("all")
        setDeliveryStatusDraft("all")
        setLatencyDraft("all")
        setReconciliationDraft("all")
        setSourceDraft("all")
        replaceParams({
            mall: null,
            deliveryStatus: null,
            latency: null,
            reconciliation: null,
            source: null,
            page: null,
        })
    }, [replaceParams])

    /** 移除单个已生效条件；chip 关闭只移除自己的条件。 */
    const removeFilter = React.useCallback(
        (key: ExecutionProjectionFilterKey) => {
            if (key === "q") setSearchDraft("")
            if (key === "mall") setMallIdDraft("all")
            if (key === "deliveryStatus") setDeliveryStatusDraft("all")
            if (key === "latency") setLatencyDraft("all")
            if (key === "reconciliation") setReconciliationDraft("all")
            if (key === "source") setSourceDraft("all")
            replaceParams({ [key]: null, page: null })
        },
        [replaceParams, setSearchDraft],
    )

    /** 同时重置草稿、面板、URL 筛选参数与分页；保留排序/视图/期间/导航上下文。 */
    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setMallIdDraft("all")
        setDeliveryStatusDraft("all")
        setLatencyDraft("all")
        setReconciliationDraft("all")
        setSourceDraft("all")
        setPanelOpen(false)
        replaceParams({
            q: null,
            mall: null,
            deliveryStatus: null,
            latency: null,
            reconciliation: null,
            source: null,
            metric: null,
            page: null,
        })
    }, [replaceParams, setSearchDraft])

    // URL 回填只同步 Draft，不抢夺用户当前的面板展开态
    React.useEffect(() => {
        setMallIdDraft(urlState.mallId)
        setDeliveryStatusDraft(urlState.deliveryStatus)
        setLatencyDraft(urlState.latency)
        setReconciliationDraft(urlState.reconciliation)
        setSourceDraft(urlState.source)
    }, [
        urlState.deliveryStatus,
        urlState.latency,
        urlState.mallId,
        urlState.reconciliation,
        urlState.source,
    ])

    return {
        q: urlState.q,
        mallId: urlState.mallId,
        deliveryStatus: urlState.deliveryStatus,
        source: urlState.source,
        latency: urlState.latency,
        reconciliation: urlState.reconciliation,
        metric: urlState.metric,
        projectionId: urlState.projectionId,
        revisionId: urlState.revisionId,
        page: urlState.page,
        pageSize: urlState.pageSize,
        hasActiveFilters: urlState.hasActiveFilters,
        replaceParams: urlState.replaceParams,
        setPageState: urlState.setPageState,
        searchDraft,
        setSearchDraft,
        searchInputRef,
        mallIdDraft,
        setMallIdDraft,
        deliveryStatusDraft,
        setDeliveryStatusDraft,
        latencyDraft,
        setLatencyDraft,
        reconciliationDraft,
        setReconciliationDraft,
        sourceDraft,
        setSourceDraft,
        panelOpen,
        setPanelOpen,
        hasStructuredFilters: hasStructuredExecutionProjectionFilters(urlState),
        applyFilters,
        clearAllFilters,
        resetMoreFilters,
        removeFilter,
    }
}

export type ExecutionProjectionFilterState = ReturnType<
    typeof useExecutionProjectionFilters
>
