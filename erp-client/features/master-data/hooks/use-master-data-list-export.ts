"use client"

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
    const [exportMeta, setExportMeta] = React.useState<ListExportMeta | null>(
        null,
    )

    const handleExport = React.useCallback(
        async (
            query: MasterDataListQuery,
            filterSnapshotLabel: string,
            fileLabel: string,
        ) => {
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
        [exportMutation],
    )

    return { exportMutation, exportMeta, handleExport }
}
