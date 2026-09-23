"use client"
import { useHistoricalDirectory } from "@/lib/historical-directory"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"
import { ApiErrorException } from "@/lib/api/errors"

import { getErrorMessage } from "@/lib/api/errors"
import { buildProfitLossColumns } from "@/features/actual-profit-loss/hooks/columns"
import {
    useCostEntriesForRowQuery,
    usePeriodBasisConfigQuery,
    useProfitLossViewQuery,
    useStartProfitLossExportMutation,
} from "@/features/actual-profit-loss/hooks/queries"
import { useProfitLossFilterPresentation } from "@/features/actual-profit-loss/hooks/use-profit-loss-filter-presentation"
import { useProfitLossUrlState } from "@/features/actual-profit-loss/hooks/use-profit-loss-url-state"
import { mapFreshnessState } from "@/features/actual-profit-loss/lib/url-state"
import type {
    CostEntryDetail,
    ProfitLossExport,
    ProfitLossRow,
} from "@/features/actual-profit-loss/types"

export type { ProfitLossAppliedChip } from "@/features/actual-profit-loss/hooks/profit-loss-filter-contract"
export type { ProfitLossUrlPatch } from "@/features/actual-profit-loss/hooks/use-profit-loss-url-state"

/**
 * 实际经营盈亏页面组合根。
 * URL、筛选草稿与服务端分页由 useProfitLossUrlState 负责；本 Hook 只编排查询、下钻和导出。
 */
export function useActualProfitLossPage() {
    const basisQuery = usePeriodBasisConfigQuery()
    const basisConfig = basisQuery.data
    const urlState = useProfitLossUrlState({
        basisConfig,
        basisResolved: basisQuery.isSuccess,
    })
    const viewQuery = useProfitLossViewQuery(
        urlState.query,
        urlState.analysisReady,
    )
    const directoryQuery = useHistoricalDirectory(
        "/admin/actual-profit-loss/history-directory",
        urlState.query,
        urlState.analysisReady,
    )
    const historyDirectory =
        directoryQuery.isError || directoryQuery.isFetching
            ? undefined
            : directoryQuery.data
    const exportMutation = useStartProfitLossExportMutation()
    const { patchUrl } = urlState
    const setPagination = React.useCallback(
        (next: import("@tanstack/react-table").PaginationState) => {
            patchUrl({
                page: next.pageIndex > 0 ? String(next.pageIndex + 1) : null,
                pageSize: next.pageSize === 20 ? null : String(next.pageSize),
                scopeVersion:
                    next.pageIndex > 0 ? viewQuery.data?.scopeVersion : null,
            })
        },
        [patchUrl, viewQuery.data?.scopeVersion],
    )
    const queryClient = useQueryClient()

    const [costDetailRow, setCostDetailRow] =
        React.useState<ProfitLossRow | null>(null)
    const [selectedCostEntryId, setSelectedCostEntryId] = React.useState<
        string | null
    >(null)
    const [exportJob, setExportJob] = React.useState<ProfitLossExport | null>(
        null,
    )
    const [exportFailed, setExportFailed] = React.useState<string | null>(null)
    const [refreshFailed, setRefreshFailed] = React.useState<string | null>(
        null,
    )
    const [refreshing, setRefreshing] = React.useState(false)
    const [scopeError, setScopeError] = React.useState<unknown>(null)
    const scopeBlockedAt = React.useRef(0)
    const costIds = costDetailRow?.costEntryIds ?? []
    const costEntriesQuery = useCostEntriesForRowQuery(
        costIds,
        viewQuery.data?.scopeVersion,
    )
    const detectedScopeError = [
        viewQuery.error,
        costEntriesQuery.error,
        exportMutation.error,
    ].find(
        (error) =>
            error instanceof ApiErrorException &&
            [401, 403, 409].includes(error.status ?? 0),
    )
    const scopeFailed = Boolean(scopeError || detectedScopeError)
    const data = scopeFailed ? undefined : viewQuery.data
    const lastScopeVersion = React.useRef<string | undefined>(undefined)
    React.useEffect(() => {
        const changed =
            lastScopeVersion.current != null &&
            lastScopeVersion.current !== data?.scopeVersion
        lastScopeVersion.current = data?.scopeVersion
        if (!scopeFailed && !changed) return
        if (detectedScopeError) {
            scopeBlockedAt.current = Date.now()
            setScopeError(detectedScopeError)
        }
        setCostDetailRow(null)
        setSelectedCostEntryId(null)
        setExportJob(null)
        queryClient.removeQueries({
            queryKey: ["actual-profit-loss"],
            type: "inactive",
        })
    }, [scopeFailed, detectedScopeError, data?.scopeVersion, queryClient])
    React.useEffect(() => {
        if (
            scopeError &&
            !detectedScopeError &&
            viewQuery.isSuccess &&
            !viewQuery.isFetching &&
            viewQuery.dataUpdatedAt > scopeBlockedAt.current
        ) {
            setScopeError(null)
        }
    }, [
        scopeError,
        detectedScopeError,
        viewQuery.isSuccess,
        viewQuery.isFetching,
        viewQuery.dataUpdatedAt,
    ])

    const rowFocusRef = React.useRef<Map<string, HTMLElement | null>>(new Map())
    const restoreFocusIdRef = React.useRef<string | null>(null)

    const filterPresentation = useProfitLossFilterPresentation({
        data,
        historyDirectory,
        qParam: urlState.qParam,
        coverage: urlState.coverage,
        customerId: urlState.customerId,
        salesOrderId: urlState.salesOrderId,
        benefitScenario: urlState.benefitScenario,
        costTypes: urlState.costTypes,
        attributionUserIds: urlState.attributionUserIds,
        attributionOrgUnitIds: urlState.attributionOrgUnitIds,
        attributionGroup: urlState.attributionGroup,
    })

    React.useEffect(() => {
        if (costDetailRow) return
        const id = restoreFocusIdRef.current
        if (!id) return
        const element = rowFocusRef.current.get(id)
        if (element) {
            element.focus()
            restoreFocusIdRef.current = null
        }
    }, [costDetailRow])

    const pageRows = React.useMemo(
        () => (data ? [...data.rows.items] : []),
        [data],
    )
    const openCostDetail = React.useCallback((row: ProfitLossRow) => {
        if (
            !row.allowedDrilldowns.includes("cost_entry") ||
            row.costEntryIds.length === 0
        ) {
            return
        }
        restoreFocusIdRef.current = row.rowId
        setCostDetailRow(row)
        setSelectedCostEntryId(row.costEntryIds[0] ?? null)
    }, [])
    /** 在原期间和筛选交集内精确下钻该历史分组，重置分页并重新授权。 */
    const openHistoricalGroup = React.useCallback(
        (row: ProfitLossRow) => {
            patchUrl({
                dimension: "sales_order",
                attributionGroup: row.rowId,
                page: null,
            })
        },
        [patchUrl],
    )
    const columns = React.useMemo(
        () =>
            buildProfitLossColumns({
                openCostDetail,
                openHistoricalGroup,
                rowFocusRef,
            }),
        [openCostDetail, openHistoricalGroup],
    )

    const freshnessUi = data
        ? mapFreshnessState(data.freshness.state, {
              refreshFailed: Boolean(refreshFailed),
              refreshing,
          })
        : { uiState: "unknown" as const, statusLabel: "等待查询" }
    const selectedEntry: CostEntryDetail | null = React.useMemo(() => {
        if (!costEntriesQuery.data || !selectedCostEntryId) return null
        return (
            costEntriesQuery.data.find(
                (entry) => entry.costEntryId === selectedCostEntryId,
            ) ?? null
        )
    }, [costEntriesQuery.data, selectedCostEntryId])

    async function handleRefresh() {
        setScopeError(null)
        exportMutation.reset()
        setCostDetailRow(null)
        setSelectedCostEntryId(null)
        if (urlState.query?.scopeVersion) {
            urlState.patchUrl({ page: null })
            return
        }
        setRefreshing(true)
        setRefreshFailed(null)
        try {
            const viewResult = await viewQuery.refetch()
            if (viewResult.error) throw viewResult.error
            const basisResult = await basisQuery.refetch()
            if (basisResult.error) throw basisResult.error
        } catch (error) {
            setRefreshFailed(
                getErrorMessage(error, "刷新失败，已保留上次成功数据。"),
            )
        } finally {
            setRefreshing(false)
        }
    }

    async function handleExport() {
        if (!data || !urlState.query || !urlState.analysisReady) return
        if (!data.fieldPermissions.canExport) return
        setExportFailed(null)
        try {
            const job = await exportMutation.mutateAsync({
                query: { ...urlState.query, scopeVersion: data.scopeVersion },
            })
            setExportJob(job)

            const url = URL.createObjectURL(
                new Blob(["\uFEFF", job.csvContent], {
                    type: "text/csv;charset=utf-8",
                }),
            )
            const anchor = document.createElement("a")
            anchor.href = url
            anchor.download = job.fileName
            anchor.click()
            URL.revokeObjectURL(url)
        } catch (error) {
            setExportFailed(
                getErrorMessage(error, "未能生成导出文件，请稍后重试。"),
            )
        }
    }

    return {
        historyDirectory,
        directoryQuery,
        basisQuery,
        basisConfig,
        viewQuery,
        exportMutation,
        costEntriesQuery,
        data,
        scopeError: scopeError || detectedScopeError,
        ...urlState,
        setPagination,
        ...filterPresentation,
        costDetailRow: scopeFailed ? null : costDetailRow,
        setCostDetailRow,
        selectedCostEntryId,
        setSelectedCostEntryId,
        exportJob,
        setExportJob,
        exportFailed,
        refreshFailed,
        handleRefresh,
        handleExport,
        pageRows,
        columns,
        freshnessUi,
        selectedEntry,
        openCostDetail,
    }
}
