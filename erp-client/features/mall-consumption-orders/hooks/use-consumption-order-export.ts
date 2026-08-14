"use client"

import * as React from "react"

import { useConsumptionOrderExportMutation } from "@/features/mall-consumption-orders/hooks/queries"
import type { ExportResultState } from "@/features/mall-consumption-orders/types"

/**
 * 导出流程状态：预览开关、创建结果与提交动作。
 * 命令负载以调用时的列表总量与筛选摘要为准（与列表页原有行为一致）。
 */
export function useConsumptionOrderExportFlow(
    rowCount: number,
    filterSummary: string,
) {
    const [exportPreviewOpen, setExportPreviewOpen] = React.useState(false)
    const [exportResult, setExportResult] =
        React.useState<ExportResultState | null>(null)

    const exportMutation = useConsumptionOrderExportMutation()

    const confirmExport = async () => {
        const requestId = `req-w25-export-${Date.now()}`
        const result = await exportMutation.mutateAsync({
            selectionSnapshotId: `snap-${requestId}`,
            fieldSetId: "w25-list-default-masked",
            requestId,
            rowCount,
            filterSummary,
        })
        setExportResult({
            jobId: result.jobId,
            rowCount: result.rowCount,
            permissionVersion: result.permissionVersion,
            maskDisclaimer: result.maskDisclaimer,
            downloadLabel: result.downloadLabel,
            expiresAt: result.expiresAt,
        })
        setExportPreviewOpen(false)
    }

    const openExportPreview = () => setExportPreviewOpen(true)
    const cancelExportPreview = () => setExportPreviewOpen(false)

    return {
        exportPreviewOpen,
        exportResult,
        exportMutation,
        confirmExport,
        openExportPreview,
        cancelExportPreview,
    }
}
