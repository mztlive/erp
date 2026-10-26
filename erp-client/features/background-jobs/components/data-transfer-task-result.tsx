"use client"

import { BusinessFailureState } from "@/components/business"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { BackgroundJobView } from "../api"
import { useDataTransferContextQuery } from "../queries"
import { TransferExportResult } from "./transfer-export-result"
import { TransferBatchResult } from "./transfer-batch-result"
import { TransferPreviewResult } from "./transfer-preview-result"

/** 文件任务按明确种类展示业务结果；上下文读取失败不伪装成通用成功。 */
export function DataTransferTaskResult({
    job,
    portal = false,
}: {
    job: BackgroundJobView
    portal?: boolean
}) {
    const context = useDataTransferContextQuery(job.id, portal)
    if (context.isError) {
        const expired =
            job.result_expires_at !== null &&
            job.result_expires_at <= Date.now() / 1000
        return (
            <div className="space-y-3">
                <BusinessFailureState
                    error={context.error}
                    action={
                        !expired ? (
                            <Button
                                id={`transfer-context-retry-${toAutomationIdSegment(job.id)}`}
                                variant="outline"
                                onClick={() => void context.refetch()}
                            >
                                重新读取任务结果
                            </Button>
                        ) : undefined
                    }
                />
                <p className="text-muted-foreground">
                    {expired
                        ? job.job_type === "export"
                            ? "历史文件与续办资料已不可用，请回到原业务列表重新查询并导出。"
                            : "历史预览或结果文件已不可用。请回到原业务页面核对已保存的结果，再决定是否重新办理，避免重复提交。"
                        : "业务信息未能读取，当前不能继续办理；请核对上方原因。权限或业务状态变化时，返回原业务页面处理。"}
                </p>
            </div>
        )
    }
    if (!context.data) {
        return <p className="text-muted-foreground">正在读取任务业务信息…</p>
    }
    const props = { job, portal, context: context.data }
    switch (context.data.kind) {
        case "inquiry_preview":
        case "inquiry_creation_preview":
        case "supplier_quote_preview":
            return (
                <TransferPreviewResult
                    {...props}
                    onChanged={() => void context.refetch()}
                />
            )
        case "supply_batch":
        case "portal_batch":
            return <TransferBatchResult {...props} />
        default:
            return <TransferExportResult {...props} />
    }
}
