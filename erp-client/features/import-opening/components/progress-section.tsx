"use client"

import {
    BackgroundJobProgress,
    BusinessEmptyState,
    DetailWorkspaceSection,
} from "@/components/business"
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
                description="开始正式导入后将在此展示任务号、成功/跳过/失败计数与最新进度。"
            />
        )
    }

    return (
        <DetailWorkspaceSection>
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
            <div className="grid gap-3 sm:grid-cols-4">
                <Fact
                    label="已处理"
                    value={`${job.processed}/${job.total}`}
                    mono
                />
                <Fact label="成功" value={job.succeeded} mono />
                <Fact label="跳过" value={job.skipped} mono />
                <Fact label="失败" value={job.failed} mono />
            </div>
        </DetailWorkspaceSection>
    )
}
