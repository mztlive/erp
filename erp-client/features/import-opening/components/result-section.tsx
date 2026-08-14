"use client"

import {
    BackgroundJobProgress,
    BatchOperationResult,
    BusinessEmptyState,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import type { ImportBatchView } from "@/features/import-opening/types"

export function ResultSection({
    batch,
    onOpenRepair,
}: {
    batch: ImportBatchView
    onOpenRepair: (batchId: string) => void
}) {
    const partitions = batch.applyPartitions

    if (!partitions && batch.stage !== "RESULT") {
        return (
            <BusinessEmptyState
                kind="no-data"
                title="结果尚未形成"
                description="导入完成后，此处将列出成功、跳过与失败项，失败的记录可在此重新处理。"
            />
        )
    }

    return (
        <div className="space-y-4">
            {partitions ? (
                <BatchOperationResult
                    title="应用结果分区"
                    succeeded={partitions.succeeded.map((i) => ({
                        id: i.id,
                        label: i.label,
                        detail: i.detail,
                        code: i.code,
                    }))}
                    skipped={partitions.skipped.map((i) => ({
                        id: i.id,
                        label: i.label,
                        detail: i.detail,
                        code: i.code,
                    }))}
                    failed={partitions.failed.map((i) => ({
                        id: i.id,
                        label: i.label,
                        detail: i.detail,
                        code: i.code,
                    }))}
                    retryAction={
                        batch.repairBatchId ? (
                            <Button
                                type="button"
                                size="sm"
                                onClick={() =>
                                    onOpenRepair(batch.repairBatchId!)
                                }
                            >
                                打开修复批次
                                {batch.repairBatchNo
                                    ? ` ${batch.repairBatchNo}`
                                    : ""}
                            </Button>
                        ) : undefined
                    }
                />
            ) : null}

            {batch.backgroundJob ? (
                <BackgroundJobProgress
                    mode="partialAllowed"
                    status={batch.backgroundJob.status}
                    total={batch.backgroundJob.total}
                    completed={batch.backgroundJob.processed}
                    succeeded={batch.backgroundJob.succeeded}
                    skipped={batch.backgroundJob.skipped}
                    failed={batch.backgroundJob.failed}
                    label="最终应用进度"
                />
            ) : null}

            <Alert>
                <AlertTitle>防重复与不可覆盖</AlertTitle>
                <AlertDescription>
                    已导入成功的数据不会因取消、重试或上传新文件而被覆盖或删除；重新处理仅针对失败项。
                </AlertDescription>
            </Alert>
        </div>
    )
}
