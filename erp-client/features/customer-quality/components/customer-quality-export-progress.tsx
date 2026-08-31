"use client"

import { BackgroundJobProgress } from "@/components/business"
import { formatDateTime } from "@/lib/datetime"
import { formatSourceWatermark } from "../lib/presentation"
import type { CustomerQualityExportJob } from "../types"

export function CustomerQualityExportProgress({
    job,
}: {
    job: CustomerQualityExportJob
}) {
    return (
        <BackgroundJobProgress
            mode="all-or-nothing"
            status={
                job.status === "queued"
                    ? "queued"
                    : job.status === "running"
                      ? "running"
                      : job.status === "succeeded"
                        ? "succeeded"
                        : "failed"
            }
            total={job.total}
            completed={job.completed}
            succeeded={job.status === "succeeded" ? job.total : undefined}
            label="客户经营质量导出"
            description={
                <>
                    期间 {job.period.from} ~ {job.period.to}。
                    {job.filterSummary}。数据更新时间{" "}
                    <span className="num">
                        {formatSourceWatermark(job.projectionWatermark)}
                    </span>
                    。{job.amountBasisNote}
                    {job.downloadLabel ? (
                        <span className="mt-1 block font-medium">
                            可下载（保留 7 天）：
                            {job.downloadLabel}
                            {job.expiresAt
                                ? ` · 失效 ${formatDateTime(job.expiresAt, "full", "passthrough")}`
                                : ""}
                        </span>
                    ) : null}
                </>
            }
        />
    )
}
