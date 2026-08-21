"use client"

import * as React from "react"

import { useMallSelectorQuery } from "@/features/entity-selectors/hooks/queries"
import type { HistoryBackfillUrlState } from "@/features/history-backfill/lib/url-state"
import {
    COST_BASIS_LABEL,
    ENVIRONMENT_LABEL,
    PROCESSING_STATUS_LABEL,
    REPORT_REVIEW_STATUS_LABEL,
    type CostBasis,
    type HistoryBackfillEnvironment,
    type HistoryBackfillProcessingStatus,
    type HistoryBackfillReportReviewStatus,
} from "@/features/history-backfill/types"

/** 可被单独移除的已生效列表条件。 */
export type JobListFilterKey =
    | "q"
    | "mallId"
    | "environment"
    | "processingStatus"
    | "reportReviewStatus"
    | "basis"

export type JobListAppliedChip = Readonly<{
    key: JobListFilterKey
    label: string
}>

/** 单个已生效条件对应的 URL 清除补丁；只清筛选参数，不碰 view 等视图参数。 */
const REMOVE_FILTER_PATCH: Record<
    JobListFilterKey,
    Partial<HistoryBackfillUrlState>
> = {
    q: { q: undefined },
    mallId: { mallId: undefined },
    environment: { environment: undefined },
    processingStatus: { processingStatus: undefined },
    reportReviewStatus: { reportReviewStatus: undefined },
    basis: { basis: undefined },
}

/**
 * 回填任务列表筛选状态（docs/ui-filter-design.md §5）：
 * Applied 只读 URL 且为唯一事实源；Draft 本地受控、提交前不请求；
 * 面板展开是纯 UI 状态。整页只有一个 apply：收起态 Enter / 搜索框尾部
 * 提交箭头与展开态「应用全部筛选」走同一个 applyFilters。
 */
export function useJobListFilters(
    urlState: HistoryBackfillUrlState,
    patchUrl: (patch: Partial<HistoryBackfillUrlState>) => void,
) {
    const q = urlState.q ?? ""
    const mallId = urlState.mallId
    const environment = urlState.environment
    const processingStatus = urlState.processingStatus
    const reportReviewStatus = urlState.reportReviewStatus
    const basis = urlState.basis

    const hasStructuredFilters = Boolean(
        mallId ||
            environment ||
            processingStatus ||
            reportReviewStatus ||
            basis,
    )

    const searchInputRef = React.useRef<HTMLInputElement | null>(null)

    // ---- Draft：本地受控，变化不触发请求 ----
    const [searchDraft, setSearchDraft] = React.useState(q)
    const [mallIdDraft, setMallIdDraft] = React.useState<string | null>(
        mallId ?? null,
    )
    const [environmentDraft, setEnvironmentDraft] = React.useState<
        HistoryBackfillEnvironment | "all"
    >(environment ?? "all")
    const [processingStatusDraft, setProcessingStatusDraft] = React.useState<
        HistoryBackfillProcessingStatus | "all"
    >(processingStatus ?? "all")
    const [reportReviewStatusDraft, setReportReviewStatusDraft] =
        React.useState<HistoryBackfillReportReviewStatus | "all">(
            reportReviewStatus ?? "all",
        )
    const [basisDraft, setBasisDraft] = React.useState<CostBasis | "all">(
        basis ?? "all",
    )

    // ---- UI 态：面板展开不写 URL ----
    const [panelOpen, setPanelOpen] = React.useState(hasStructuredFilters)

    /** 一次性提交全部草稿；默认值转为参数缺省，页码归 1。 */
    const applyFilters = React.useCallback(() => {
        patchUrl({
            q: searchDraft.trim() || undefined,
            mallId: mallIdDraft ?? undefined,
            environment:
                environmentDraft === "all" ? undefined : environmentDraft,
            processingStatus:
                processingStatusDraft === "all"
                    ? undefined
                    : processingStatusDraft,
            reportReviewStatus:
                reportReviewStatusDraft === "all"
                    ? undefined
                    : reportReviewStatusDraft,
            basis: basisDraft === "all" ? undefined : basisDraft,
            page: 1,
        })
        setPanelOpen(false)
    }, [
        basisDraft,
        environmentDraft,
        mallIdDraft,
        patchUrl,
        processingStatusDraft,
        reportReviewStatusDraft,
        searchDraft,
    ])

    /** 移除单个已生效条件并同步对应草稿。 */
    const removeFilter = React.useCallback(
        (key: JobListFilterKey) => {
            if (key === "q") setSearchDraft("")
            if (key === "mallId") setMallIdDraft(null)
            if (key === "environment") setEnvironmentDraft("all")
            if (key === "processingStatus") setProcessingStatusDraft("all")
            if (key === "reportReviewStatus") setReportReviewStatusDraft("all")
            if (key === "basis") setBasisDraft("all")
            patchUrl({ ...REMOVE_FILTER_PATCH[key], page: 1 })
        },
        [patchUrl],
    )

    /** 只清结构化条件：保留关键词，保持面板展开，并回第 1 页。 */
    const resetMoreFilters = React.useCallback(() => {
        setMallIdDraft(null)
        setEnvironmentDraft("all")
        setProcessingStatusDraft("all")
        setReportReviewStatusDraft("all")
        setBasisDraft("all")
        patchUrl({
            mallId: undefined,
            environment: undefined,
            processingStatus: undefined,
            reportReviewStatus: undefined,
            basis: undefined,
            page: 1,
        })
    }, [patchUrl])

    /** 同时重置 Draft、面板、全部筛选参数与分页；保留 view 等视图与导航参数。 */
    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setMallIdDraft(null)
        setEnvironmentDraft("all")
        setProcessingStatusDraft("all")
        setReportReviewStatusDraft("all")
        setBasisDraft("all")
        setPanelOpen(false)
        patchUrl({
            q: undefined,
            mallId: undefined,
            environment: undefined,
            processingStatus: undefined,
            reportReviewStatus: undefined,
            basis: undefined,
            page: 1,
        })
    }, [patchUrl])

    // ---- URL 回填草稿；关键词输入聚焦时保护未提交草稿 ----
    React.useEffect(() => {
        if (document.activeElement !== searchInputRef.current) {
            setSearchDraft(urlState.q ?? "")
        }
    }, [urlState.q])

    React.useEffect(() => {
        setMallIdDraft(urlState.mallId ?? null)
        setEnvironmentDraft(urlState.environment ?? "all")
        setProcessingStatusDraft(urlState.processingStatus ?? "all")
        setReportReviewStatusDraft(urlState.reportReviewStatus ?? "all")
        setBasisDraft(urlState.basis ?? "all")
    }, [
        urlState.basis,
        urlState.environment,
        urlState.mallId,
        urlState.processingStatus,
        urlState.reportReviewStatus,
    ])

    // ---- `/` 聚焦搜索；输入框 / 文本域 / 弹层打开时不抢焦点 ----
    React.useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            if (
                event.key !== "/" ||
                event.target instanceof HTMLInputElement ||
                event.target instanceof HTMLTextAreaElement
            ) {
                return
            }
            if (
                document.querySelector(
                    '[role="dialog"], [data-slot="sheet"]',
                )
            ) {
                return
            }
            event.preventDefault()
            searchInputRef.current?.focus()
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [])

    // ---- 已生效条件与摘要：只读 Applied ----
    const mallOptionsQuery = useMallSelectorQuery("filter")
    const selectedMallName = React.useMemo(
        () =>
            mallOptionsQuery.data?.find((mall) => mall.id === mallId)?.name ??
            mallId,
        [mallId, mallOptionsQuery.data],
    )

    const appliedChips = React.useMemo<readonly JobListAppliedChip[]>(() => {
        const chips: JobListAppliedChip[] = []
        if (q.trim()) chips.push({ key: "q", label: `搜索：${q.trim()}` })
        if (mallId) {
            chips.push({ key: "mallId", label: `商城：${selectedMallName}` })
        }
        if (environment) {
            chips.push({
                key: "environment",
                label: `环境：${ENVIRONMENT_LABEL[environment]}`,
            })
        }
        if (processingStatus) {
            chips.push({
                key: "processingStatus",
                label: `处理状态：${PROCESSING_STATUS_LABEL[processingStatus]}`,
            })
        }
        if (reportReviewStatus) {
            chips.push({
                key: "reportReviewStatus",
                label: `报告确认：${REPORT_REVIEW_STATUS_LABEL[reportReviewStatus]}`,
            })
        }
        if (basis) {
            chips.push({
                key: "basis",
                label: `成本口径：${COST_BASIS_LABEL[basis]}`,
            })
        }
        return chips
    }, [
        basis,
        environment,
        mallId,
        processingStatus,
        q,
        reportReviewStatus,
        selectedMallName,
    ])

    const hasActiveFilters = q.trim() !== "" || hasStructuredFilters

    const tableDescription = React.useMemo(() => {
        if (appliedChips.length === 0) return "处理状态与报告确认状态分列"
        return `已按 ${appliedChips.map((chip) => chip.label).join("、")} 筛选`
    }, [appliedChips])

    return {
        searchInputRef,
        q,
        mallId,
        environment,
        processingStatus,
        reportReviewStatus,
        basis,
        hasStructuredFilters,
        hasActiveFilters,
        searchDraft,
        setSearchDraft,
        mallIdDraft,
        setMallIdDraft,
        environmentDraft,
        setEnvironmentDraft,
        processingStatusDraft,
        setProcessingStatusDraft,
        reportReviewStatusDraft,
        setReportReviewStatusDraft,
        basisDraft,
        setBasisDraft,
        panelOpen,
        setPanelOpen,
        appliedChips,
        tableDescription,
        applyFilters,
        removeFilter,
        resetMoreFilters,
        clearAllFilters,
    }
}
