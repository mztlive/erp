"use client"

import {
    BackgroundJobProgress,
    BusinessEmptyState,
    DetailWorkspaceFact,
    DetailWorkspaceFacts,
    DetailWorkspaceSection,
} from "@/components/business"
import type { ImportBatchView } from "@/features/import-opening/types"
import { formatDateTime } from "@/lib/datetime"

export function ProgressSection({ batch }: { batch: ImportBatchView }) {
    const job = batch.backgroundJob
    if (!job) {
        return (
            <DetailWorkspaceSection title="执行进度" width="form">
                <BusinessEmptyState
                    kind="no-data"
                    title="暂无导入任务"
                    description="开始正式导入后将在此展示任务号、成功/跳过/失败计数与最新进度。"
                    className="rounded-lg border-0 bg-transparent shadow-none ring-0"
                />
            </DetailWorkspaceSection>
        )
    }

    return (
        <DetailWorkspaceSection title="执行进度">
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
                        <span className="num">
                            {formatDateTime(
                                job.updatedAt,
                                "dateStyle",
                                "passthrough",
                            )}
                        </span>{" "}
                        ·
                        允许部分成功；已形成的处理结果不会因同批其它失败而回退。
                    </span>
                }
            />
            <DetailWorkspaceFacts columns={4}>
                <DetailWorkspaceFact
                    label="已处理"
                    value={`${job.processed}/${job.total}`}
                    numeric
                />
                <DetailWorkspaceFact
                    label="成功"
                    value={job.succeeded}
                    numeric
                />
                <DetailWorkspaceFact label="跳过" value={job.skipped} numeric />
                <DetailWorkspaceFact label="失败" value={job.failed} numeric />
            </DetailWorkspaceFacts>
        </DetailWorkspaceSection>
    )
}
