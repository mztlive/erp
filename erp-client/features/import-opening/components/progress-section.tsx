"use client"

import {
    BackgroundJobProgress,
    BusinessEmptyState,
    surfacePanelClassName,
} from "@/components/business"
import {
    Card,
    CardContent,
} from "@/components/ui/card"
import { Fact } from "@/features/import-opening/components/batch-facts"
import type { ImportBatchView } from "@/features/import-opening/types"
import { formatDateTime } from "@/lib/datetime"

export function ProgressSection({ batch }: { batch: ImportBatchView }) {
    const job = batch.backgroundJob
    if (!job) {
        return (
            <BusinessEmptyState
                kind="no-data"
                title="暂无导入任务"
                description="提交应用后将在此展示任务号、成功/跳过/失败计数与最近进度时间；刷新可恢复进度。"
            />
        )
    }

    return (
        <div className="space-y-4">
            <BackgroundJobProgress
                mode="partialAllowed"
                status={job.status}
                total={job.total}
                completed={job.processed}
                succeeded={job.succeeded}
                skipped={job.skipped}
                failed={job.failed}
                label={`导入执行进度 · ${batch.batchNo}`}
                description={
                    <span>
                        最近进度{" "}
                        {formatDateTime(
                            job.updatedAt,
                            "dateStyle",
                            "passthrough",
                        )}{" "}
                        ·
                        允许部分成功；已形成的处理结果不会因同批其它失败而回退。
                    </span>
                }
            />
            <Card size="sm" className={surfacePanelClassName}>
                <CardContent className="grid gap-3 pt-4 sm:grid-cols-4">
                    <Fact
                        label="已处理"
                        value={`${job.processed}/${job.total}`}
                        mono
                    />
                    <Fact label="成功" value={job.succeeded} mono />
                    <Fact label="跳过" value={job.skipped} mono />
                    <Fact label="失败" value={job.failed} mono />
                </CardContent>
            </Card>
        </div>
    )
}
