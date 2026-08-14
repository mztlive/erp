"use client"

import * as React from "react"

import { useSupplierOrderExportMutation } from "@/features/supplier-orders/hooks/queries"
import type {
    ExportCommand,
    ExportJobResult,
} from "@/features/supplier-orders/types"

/**
 * 列表导出流程：
 * - 预览开关 + 待执行命令（失败可按原快照重试）+ 成功结果展示；
 * - 提交走 useSupplierOrderExportMutation（成功后失效列表缓存）。
 */
export function useSupplierOrdersExport() {
    const [exportPreviewOpen, setExportPreviewOpen] = React.useState(false)
    const [pendingExport, setPendingExport] =
        React.useState<ExportCommand | null>(null)
    const [exportResult, setExportResult] =
        React.useState<ExportJobResult | null>(null)

    const exportMutation = useSupplierOrderExportMutation()

    const runExport = React.useCallback(
        async (command: ExportCommand) => {
            const result = await exportMutation.mutateAsync(command)
            setExportResult(result)
            setPendingExport(null)
            setExportPreviewOpen(false)
        },
        [exportMutation],
    )

    const openExportPreview = React.useCallback(
        () => setExportPreviewOpen(true),
        [],
    )
    const closeExportPreview = React.useCallback(
        () => setExportPreviewOpen(false),
        [],
    )

    const confirmExport = React.useCallback(
        async (input: { total: number; filterSummary: string }) => {
            const requestId = `req-w26-export-${Date.now()}`
            const command: ExportCommand = {
                selectionSnapshotId: `snap-${requestId}`,
                fieldSetId: "w26-list-default-masked",
                requestId,
                rowCount: input.total,
                filterSummary: input.filterSummary,
            }
            setPendingExport(command)
            await runExport(command)
        },
        [runExport],
    )

    const retryExport = React.useCallback(async () => {
        if (!pendingExport) return
        await runExport(pendingExport)
    }, [pendingExport, runExport])

    return {
        exportPreviewOpen,
        exportResult,
        pendingExport,
        exportMutation,
        openExportPreview,
        closeExportPreview,
        confirmExport,
        retryExport,
    }
}
