"use client"
import { useMutation } from "@tanstack/react-query"
import { useState } from "react"
import { toast } from "@/components/ui/toast"
import { runDataTransfer, downloadTransfer } from "@/lib/data-transfer"
import { getErrorMessage } from "@/lib/api/errors"
import { sellableListParams } from "../api/lists/sellable"
import type { MasterDataListQuery } from "../types"
import type { ListExportMeta } from "./use-master-data-list-export"
export function useSellableExcelExport() {
    const [exportMeta, setExportMeta] = useState<ListExportMeta | null>(null)
    const mutation = useMutation({
        mutationFn: async (input: {
            query: MasterDataListQuery
            selectedIds?: ReadonlySet<string>
            filterSnapshotLabel: string
            fileLabel: string
        }) => {
            if (input.selectedIds?.size === 0) return false
            const file = await runDataTransfer("product_pool_excel", {
                query: {
                    ...sellableListParams(input.query),
                    max_supplier_count:
                        input.query.sellableSupplyPreset === "single-supplier"
                            ? 1
                            : undefined,
                },
                selected_ids: input.selectedIds
                    ? [...input.selectedIds]
                    : undefined,
            })
            downloadTransfer(file)
            setExportMeta({
                jobId: file.jobId,
                rowCount:
                    typeof file.metadata?.row_count === "number"
                        ? file.metadata.row_count
                        : (input.selectedIds?.size ?? 0),
                filterSnapshotLabel: input.filterSnapshotLabel,
            })
            return true
        },
    })
    return {
        exportMeta,
        pending: mutation.isPending,
        handleExcelExport: async (
            input: Parameters<typeof mutation.mutateAsync>[0],
        ) => {
            try {
                return await mutation.mutateAsync(input)
            } catch (error) {
                toast.add({
                    title: "导出失败",
                    description: getErrorMessage(error, "请查看原任务后重试"),
                    type: "error",
                })
                return false
            }
        },
    }
}
