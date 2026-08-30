"use client"

import * as React from "react"

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
import { buildProfitLossCsv } from "@/features/actual-profit-loss/lib/csv"
import { mapFreshnessState } from "@/features/actual-profit-loss/lib/url-state"
import type {
    CostEntryDetail,
    ProfitLossExportJob,
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
    const exportMutation = useStartProfitLossExportMutation()
    const data = viewQuery.data

    const [costDetailRow, setCostDetailRow] =
        React.useState<ProfitLossRow | null>(null)
    const [selectedCostEntryId, setSelectedCostEntryId] = React.useState<
        string | null
    >(null)
    const [exportJob, setExportJob] =
        React.useState<ProfitLossExportJob | null>(null)
    const [exportFailed, setExportFailed] = React.useState<string | null>(null)
    const [refreshFailed, setRefreshFailed] = React.useState<string | null>(
        null,
    )
    const [refreshing, setRefreshing] = React.useState(false)
    const rowFocusRef = React.useRef<Map<string, HTMLElement | null>>(new Map())
    const restoreFocusIdRef = React.useRef<string | null>(null)

    const costIds = costDetailRow?.costEntryIds ?? []
    const costEntriesQuery = useCostEntriesForRowQuery(costIds)
    const filterPresentation = useProfitLossFilterPresentation({
        data,
        qParam: urlState.qParam,
        coverage: urlState.coverage,
        customerId: urlState.customerId,
        salesOrderId: urlState.salesOrderId,
        benefitScenario: urlState.benefitScenario,
        fulfillmentModes: urlState.fulfillmentModes,
        costTypes: urlState.costTypes,
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
    const columns = React.useMemo(
        () => buildProfitLossColumns({ openCostDetail, rowFocusRef }),
        [openCostDetail],
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
                query: urlState.query,
                view: data,
                coverage: urlState.coverage,
            })
            setExportJob(job)

            const csv = buildProfitLossCsv(
                data,
                job.watermark,
                urlState.coverage,
            )
            const url = URL.createObjectURL(
                new Blob(["\uFEFF", csv], {
                    type: "text/csv;charset=utf-8",
                }),
            )
            const anchor = document.createElement("a")
            anchor.href = url
            anchor.download = `实际盈亏-非卡券不含税-${job.watermark.periodFrom}_${job.watermark.periodTo}.csv`
            anchor.click()
            URL.revokeObjectURL(url)
        } catch (error) {
            setExportFailed(
                getErrorMessage(error, "未能生成导出文件，请稍后重试。"),
            )
        }
    }

    return {
        basisQuery,
        basisConfig,
        viewQuery,
        exportMutation,
        costEntriesQuery,
        data,
        ...urlState,
        ...filterPresentation,
        costDetailRow,
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
