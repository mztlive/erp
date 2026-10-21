"use client"

import * as React from "react"

import { toast } from "@/components/ui/toast"
import { useMasterDataExportMutation } from "@/features/master-data/hooks/queries"
import type { ListExportMeta } from "@/features/master-data/hooks/use-master-data-list-export"
import { selectSellableRows } from "@/features/master-data/lib/sellable-excel-rows"
import { getErrorPresentation } from "@/lib/api/errors"

export function useSellableExcelExport() {
    const exportMutation = useMasterDataExportMutation()
    const [exportMeta, setExportMeta] = React.useState<ListExportMeta | null>(
        null,
    )
    const [pending, setPending] = React.useState(false)

    const handleExcelExport = React.useCallback(
        async (input: {
            query: Parameters<typeof exportMutation.mutateAsync>[0]
            selectedIds?: ReadonlySet<string>
            filterSnapshotLabel: string
            fileLabel: string
        }) => {
            if (input.selectedIds?.size === 0) return false
            setPending(true)
            try {
                const refreshed = await exportMutation.mutateAsync(input.query)
                const rows = input.selectedIds
                    ? selectSellableRows(refreshed.rows, input.selectedIds)
                    : [...refreshed.rows]
                if (input.selectedIds && rows.length !== input.selectedIds.size)
                    throw new Error(
                        "部分勾选商品已不可导出，请刷新商品池后重新选择",
                    )
                if (rows.length === 0) {
                    toast.add({
                        title: "没有可导出的商品",
                        description:
                            "勾选的商品已不在当前结果中，请重新勾选后再导出。",
                        type: "warning",
                    })
                    return false
                }
                const { buildSellableItemsExcelFile } =
                    await import("@/features/master-data/lib/export-sellable-excel")
                await buildSellableItemsExcelFile({
                    rows,
                    filterSnapshotLabel: input.filterSnapshotLabel,
                    fileLabel: input.fileLabel,
                })
                const datePart = new Date()
                    .toISOString()
                    .slice(0, 10)
                    .replaceAll("-", "")
                setExportMeta({
                    jobId: `导出-${datePart}-${String(Date.now() % 100000).padStart(5, "0")}`,
                    rowCount: rows.length,
                    filterSnapshotLabel: input.filterSnapshotLabel,
                })
                return true
            } catch (error) {
                const presentation = getErrorPresentation(
                    error,
                    "导出未完成，请稍后重试。",
                )
                toast.add({
                    title: presentation.title,
                    description: presentation.description,
                    type: "error",
                })
                return false
            } finally {
                setPending(false)
            }
        },
        [exportMutation],
    )

    return { exportMeta, pending, handleExcelExport }
}
