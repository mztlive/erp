"use client"

import { useMutation } from "@tanstack/react-query"
import { runDataTransfer, downloadTransfer } from "@/lib/data-transfer"
import { masterTransferQuery } from "../api/transfer-query"
import * as React from "react"

import { useMasterDataExportMutation } from "@/features/master-data/hooks/queries"
import {
    buildMasterDataExportCsv,
    downloadCsv,
} from "@/features/master-data/lib/export-csv"
import type { MasterDataListQuery } from "@/features/master-data/types"

export type ListExportMeta = {
    jobId: string
    rowCount: number
    filterSnapshotLabel: string
}

/** 按当前筛选重新拉列表并下载 CSV。 */
export function useMasterDataListExport() {
    const exportMutation = useMasterDataExportMutation()
    const background = useMutation({
        mutationFn: (
            query: NonNullable<ReturnType<typeof masterTransferQuery>>,
        ) => runDataTransfer("master_data", query),
    })
    const [exportMeta, setExportMeta] = React.useState<ListExportMeta | null>(
        null,
    )

    const handleExport = React.useCallback(
        async (
            query: MasterDataListQuery,
            filterSnapshotLabel: string,
            fileLabel: string,
        ) => {
            const task = masterTransferQuery(query)
            if (task) {
                const result = await background.mutateAsync(task)
                downloadTransfer(result)
                setExportMeta({
                    jobId: result.jobId,
                    rowCount:
                        typeof result.metadata?.row_count === "number"
                            ? result.metadata.row_count
                            : 0,
                    filterSnapshotLabel,
                })
                return
            }
            const refreshed = await exportMutation.mutateAsync(query)
            if (refreshed.rows.length === 0) return
            const csv = buildMasterDataExportCsv(
                refreshed.rows,
                filterSnapshotLabel,
            )
            downloadCsv(csv, `基础资料-${fileLabel}`)
            const datePart = new Date()
                .toISOString()
                .slice(0, 10)
                .replace(/-/g, "")
            setExportMeta({
                jobId: `导出-${datePart}-${String(Date.now() % 100000).padStart(5, "0")}`,
                rowCount: refreshed.rows.length,
                filterSnapshotLabel,
            })
        },
        [exportMutation, background],
    )

    return {
        exportMutation: {
            ...exportMutation,
            isPending: exportMutation.isPending || background.isPending,
        },
        exportMeta,
        handleExport,
    }
}
