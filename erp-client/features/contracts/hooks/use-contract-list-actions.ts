"use client"

import * as React from "react"

import { toast } from "@/components/ui/toast"
import { useCreateContractExportJobMutation } from "@/features/contracts/hooks/queries"
import type {
    ContractExportJob,
    UploadContractPdfResult,
} from "@/features/contracts/types"

export type ContractActionResult = {
    status: "succeeded" | "blocked"
    title: string
    description: string
    facts?: Array<{ label: string; value: string }>
    nextHref?: string
    createSalesOrderHref?: string
}

/** 列表页动作结果：上传用短暂 Toast；导出保留页内 FormalActionResult。 */
export function useContractListActions({
    filteredCount,
    filterSnapshotLabel,
}: {
    filteredCount: number
    filterSnapshotLabel: string
}) {
    const [exportJob, setExportJob] = React.useState<ContractExportJob | null>(
        null,
    )
    const [actionResult, setActionResult] =
        React.useState<ContractActionResult | null>(null)
    const [highlightedContractId, setHighlightedContractId] = React.useState<
        string | null
    >(null)

    const exportMutation = useCreateContractExportJobMutation()

    const handleUploadSuccess = React.useCallback(
        (result: UploadContractPdfResult) => {
            toast.add({
                title: "合同 PDF 已归档",
                description: `${result.contractNo} · v${result.revisionNo} 已可引用建单`,
                type: "success",
                timeout: 4000,
            })
            setHighlightedContractId(result.contractId)
            setActionResult(null)
        },
        [],
    )

    const handleExport = React.useCallback(async () => {
        if (filteredCount === 0) return
        const job = await exportMutation.mutateAsync({
            rowCount: filteredCount,
            filterSnapshotLabel,
        })
        setExportJob(job)
        setActionResult({
            status: "succeeded",
            title: "导出完成",
            description:
                "已生成 CSV 文件，内容按当前筛选生成；下载时将重新校验权限。",
            facts: [
                { label: "筛选结果", value: job.filterSnapshotLabel },
                { label: "行数", value: String(job.rowCount) },
                { label: "文件", value: job.downloadLabel },
            ],
        })
    }, [exportMutation, filterSnapshotLabel, filteredCount])

    return {
        exportJob,
        actionResult,
        highlightedContractId,
        exportPending: exportMutation.isPending,
        handleUploadSuccess,
        handleExport,
    }
}
