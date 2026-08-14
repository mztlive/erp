"use client"

import { BackgroundJobProgress } from "@/components/business"
import type { BulkProjectionJob } from "@/features/execution-projections/types"

export function BulkProjectionJobProgress({ job }: { job: BulkProjectionJob }) {
    return (
        <BackgroundJobProgress
            mode="partialAllowed"
            status={job.status}
            total={job.total}
            completed={job.completed}
            succeeded={job.succeeded}
            skipped={job.skipped + job.stillUnknown}
            failed={job.failed}
            label={
                job.action === "BULK_RETRY" ? "批量重试任务" : "批量查询任务"
            }
            description={
                <>
                    本次选择共 {job.total} 项。成功 {job.succeeded} · 跳过{" "}
                    {job.skipped} · 仍未知 {job.stillUnknown} · 失败{" "}
                    {job.failed}。
                    {job.stillUnknown > 0
                        ? " 仍未知项未按成功处理、未计入已确认。"
                        : null}
                </>
            }
        />
    )
}
