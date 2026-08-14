"use client"

import * as React from "react"

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
}

/** 列表页动作结果：上传归档反馈 + 导出任务反馈。 */
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

    const exportMutation = useCreateContractExportJobMutation()

    const handleUploadSuccess = React.useCallback(
        (result: UploadContractPdfResult) => {
            setActionResult({
                status: "succeeded",
                title: "合同 PDF 已归档",
                description:
                    "已形成可追溯的合同版本，可直接选择用于新建销售单。",
                facts: [
                    { label: "合同号", value: result.contractNo },
                    { label: "修订", value: `v${result.revisionNo}` },
                    { label: "文件", value: result.fileName },
                    {
                        label: "上传时间",
                        value: result.uploadedAt.slice(0, 19).replace("T", " "),
                    },
                    { label: "下一步", value: "查看详情核对或新建销售单" },
                ],
                nextHref: `/sales/contracts/${result.contractId}`,
            })
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
        exportPending: exportMutation.isPending,
        handleUploadSuccess,
        handleExport,
    }
}
