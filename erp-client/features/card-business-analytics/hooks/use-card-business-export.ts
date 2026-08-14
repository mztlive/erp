"use client"

import * as React from "react"

import type {
    CardBusinessAnalyticsQuery,
    CardBusinessAnalyticsView,
    CardBusinessExportJob,
} from "../types"
import { useStartCardBusinessExportMutation } from "./queries"

/**
 * 导出任务状态簇：预览开关、任务结果与确认导出。
 * 提交副作用经 TanStack Query mutation（见 ./queries）。
 */
export function useCardBusinessExport(args: {
    data: CardBusinessAnalyticsView | undefined
    analysisQuery: CardBusinessAnalyticsQuery | null
}) {
    const { data, analysisQuery } = args
    const exportMutation = useStartCardBusinessExportMutation()
    const [exportJob, setExportJob] =
        React.useState<CardBusinessExportJob | null>(null)
    const [exportPreviewOpen, setExportPreviewOpen] = React.useState(false)

    async function handleExportConfirm() {
        if (!data || !analysisQuery) return
        setExportPreviewOpen(false)
        const job = await exportMutation.mutateAsync({
            query: analysisQuery,
            view: {
                period: data.period,
                scope: data.scope,
                freshness: data.freshness,
                coverage: data.coverage,
                filterSummary: data.filterSummary,
                wechatExcludedNote: data.wechatExcludedNote,
                fieldPermissions: data.fieldPermissions,
                rows: data.rows,
            },
        })
        setExportJob(job)
    }

    return {
        exportJob,
        setExportJob,
        exportPreviewOpen,
        setExportPreviewOpen,
        handleExportConfirm,
        isExporting: exportMutation.isPending,
    }
}
