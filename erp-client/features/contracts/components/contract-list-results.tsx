"use client"

import { FormalActionResult } from "@/components/business"
import type { ContractActionResult } from "@/features/contracts/hooks/use-contract-list-actions"
import type { ContractExportJob } from "@/features/contracts/types"

type ContractListResultsProps = {
    actionResult: ContractActionResult | null
    exportJob: ContractExportJob | null
}

/** 列表页导出反馈条；上传成功改为短暂 Toast，不在此占用页顶。 */
export function ContractListResults({
    actionResult,
    exportJob,
}: ContractListResultsProps) {
    return (
        <>
            {actionResult ? (
                <FormalActionResult
                    status={actionResult.status}
                    title={actionResult.title}
                    description={actionResult.description}
                    facts={actionResult.facts}
                />
            ) : null}

            {exportJob && !actionResult ? (
                <FormalActionResult
                    status="succeeded"
                    title="合同导出完成"
                    description={`共 ${exportJob.rowCount} 条，已按当前筛选下载完整 CSV。`}
                    facts={[
                        { label: "文件", value: exportJob.downloadLabel },
                        { label: "行数", value: String(exportJob.rowCount) },
                    ]}
                />
            ) : null}
        </>
    )
}
