"use client"

import { ChevronDownIcon } from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"
import {
    CATALOG_JOB_LABEL,
    catalogRunning,
    type CatalogJob,
    type CatalogResult,
} from "../../api/catalog"

function JobStatus({ job }: { job: CatalogJob }) {
    const variant = job.stalled
        ? "warning"
        : job.status === "failed"
          ? "destructive"
          : job.status === "succeeded"
            ? "success"
            : catalogRunning(job)
              ? "info"
              : "neutral"
    return (
        <Badge variant={variant}>
            {job.stalled ? "同步已中断" : CATALOG_JOB_LABEL[job.status]}
        </Badge>
    )
}

function jobTime(timestamp: number) {
    return formatDateTime(new Date(timestamp * 1000).toISOString())
}

function JobDetails({ job }: { job: CatalogJob }) {
    return (
        <div className="space-y-4 text-sm">
            <dl className="grid gap-2 text-xs text-muted-foreground">
                <div className="flex flex-wrap gap-x-3 gap-y-1">
                    <dt>同步编号</dt>
                    <dd className="num break-all">{job.job_no}</dd>
                </div>
                {job.finished_at !== null ? (
                    <div className="flex flex-wrap gap-x-3 gap-y-1">
                        <dt>结束时间</dt>
                        <dd className="num">{jobTime(job.finished_at)}</dd>
                    </div>
                ) : null}
            </dl>
            {job.error_summary ? (
                <div className="space-y-1.5 rounded-lg bg-muted/50 p-3">
                    <p className="font-medium">失败原因</p>
                    <p className="break-words text-muted-foreground leading-relaxed">
                        {job.error_summary}
                    </p>
                </div>
            ) : null}
            {job.skipped_count > 0 ? (
                <div className="space-y-2">
                    <p className="font-medium">
                        已跳过 {job.skipped_count} 项异常
                    </p>
                    <p className="text-xs text-muted-foreground">
                        展示最近 {job.skipped_samples.length}{" "}
                        条；修正来源数据后重新同步可补齐。
                    </p>
                    <ol className="divide-y rounded-lg border px-3">
                        {job.skipped_samples.map((message, index) => (
                            <li
                                key={`${job.job_id}-${index}`}
                                className="flex gap-3 py-3"
                            >
                                <span className="num shrink-0 text-xs text-muted-foreground">
                                    {index + 1}.
                                </span>
                                <p className="min-w-0 break-words text-xs leading-relaxed text-muted-foreground">
                                    {message}
                                </p>
                            </li>
                        ))}
                    </ol>
                </div>
            ) : null}
        </div>
    )
}

export function CatalogSyncRecords({ data }: { data: CatalogResult }) {
    const [latest, ...history] = data.jobs
    return (
        <div className="space-y-6">
            {latest ? (
                <section
                    aria-label="最近一次同步"
                    className="space-y-4 rounded-xl border p-4"
                >
                    <div className="flex flex-wrap items-start justify-between gap-3">
                        <div className="space-y-1">
                            <h3 className="text-sm font-medium">
                                最近一次同步
                            </h3>
                            <p className="num text-xs text-muted-foreground">
                                {jobTime(latest.created_at)} 开始
                            </p>
                        </div>
                        <JobStatus job={latest} />
                    </div>
                    <dl className="grid grid-cols-3 divide-x rounded-lg bg-muted/50 py-3">
                        {[
                            ["已读取规格", latest.observed_count],
                            ["已读取页数", latest.pages],
                            ["跳过异常", latest.skipped_count],
                        ].map(([label, count]) => (
                            <div key={label} className="min-w-0 space-y-1 px-3">
                                <dt className="text-xs text-muted-foreground">
                                    {label}
                                </dt>
                                <dd className="num text-xl font-semibold">
                                    {count}
                                </dd>
                            </div>
                        ))}
                    </dl>
                    {catalogRunning(latest) ? (
                        <p
                            role="status"
                            className="text-xs text-muted-foreground"
                        >
                            后台同步中，可离开页面后再查看进度。
                        </p>
                    ) : null}
                    <details className="group border-t pt-3">
                        <summary
                            id={`supplier-catalog-sync-${toAutomationIdSegment(latest.job_id)}-details`}
                            className="flex cursor-pointer list-none items-center justify-between gap-2 text-sm font-medium [&::-webkit-details-marker]:hidden"
                        >
                            {latest.error_summary || latest.skipped_count > 0
                                ? "查看异常详情"
                                : "查看同步详情"}
                            <ChevronDownIcon
                                aria-hidden="true"
                                className="size-4 text-muted-foreground transition-transform group-open:rotate-180"
                            />
                        </summary>
                        <div className="pt-4">
                            <JobDetails job={latest} />
                        </div>
                    </details>
                </section>
            ) : (
                <p className="py-8 text-center text-sm text-muted-foreground">
                    尚无同步记录
                </p>
            )}
            <p className="text-xs leading-relaxed text-muted-foreground">
                {data.incremental_supported
                    ? "支持增量更新"
                    : "采用全量读取校对"}
                {" · "}
                {data.coverage === "COMPLETE"
                    ? "覆盖供应商授权的完整目录"
                    : "仅覆盖部分目录，未返回的商品不能视为下架"}
            </p>
            {history.length > 0 ? (
                <section aria-label="历史同步记录" className="space-y-2">
                    <h3 className="text-sm font-medium">
                        历史记录{" "}
                        <span className="ml-1 font-normal text-muted-foreground">
                            {history.length}
                        </span>
                    </h3>
                    <div className="divide-y">
                        {history.map((job) => (
                            <details key={job.job_id} className="group py-4">
                                <summary
                                    id={`supplier-catalog-sync-${toAutomationIdSegment(job.job_id)}-details`}
                                    className="flex cursor-pointer list-none items-center gap-3 [&::-webkit-details-marker]:hidden"
                                >
                                    <div className="min-w-0 flex-1 space-y-2">
                                        <div className="flex flex-wrap items-center gap-2">
                                            <span className="num text-sm font-medium">
                                                {jobTime(job.created_at)}
                                            </span>
                                            <JobStatus job={job} />
                                        </div>
                                        <p className="text-xs text-muted-foreground">
                                            读取 {job.observed_count} 条规格 ·{" "}
                                            {job.pages} 页 · 跳过{" "}
                                            {job.skipped_count} 项
                                        </p>
                                    </div>
                                    <ChevronDownIcon
                                        aria-hidden="true"
                                        className="size-4 shrink-0 text-muted-foreground transition-transform group-open:rotate-180"
                                    />
                                </summary>
                                <div className="pt-4">
                                    <JobDetails job={job} />
                                </div>
                            </details>
                        ))}
                    </div>
                </section>
            ) : null}
        </div>
    )
}
