"use client"

import { useState } from "react"
import Link from "next/link"
import { ArrowUpRightIcon, DownloadIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { Progress, ProgressLabel } from "@/components/ui/progress"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { hasPermission } from "@/lib/permissions"
import { cn } from "@/lib/utils"
import type { BackgroundJobItemStatus, BackgroundJobView } from "../api"
import { PRODUCT_IMPORT_RESULT_LABELS } from "../lib/product-import-results"
import {
    useBackgroundJobItemsQuery,
    useProductImportResultsDownload,
} from "../queries"
import { BackgroundJobItemResults } from "./background-job-item-results"

/** 优先显示需核对的导入行，下载使用同一个结果分类。 */
export function ProductImportResult({
    job,
    canReadItems,
    currentUserId,
}: {
    job: BackgroundJobView
    canReadItems: boolean
    currentUserId?: string
}) {
    const [selection, setSelection] = useState<{
        status: BackgroundJobItemStatus
        page: number
    }>()
    const account = useAccountProfileQuery()
    const permissions = account.data?.permissions
    const canList = hasPermission(permissions, "product:list")
    const canDownload =
        canReadItems &&
        job.requested_by === currentUserId &&
        canList &&
        hasPermission(permissions, "product:create")
    const finished = job.finished_at !== null
    const counts = {
        failed: job.failed_count,
        unprocessed: Math.max(0, job.total_count - job.processed_count),
        success: job.success_count,
        skipped: job.skipped_count,
    }
    const statuses = Object.keys(counts) as BackgroundJobItemStatus[]
    const status =
        selection?.status ??
        statuses.find((status) => counts[status] > 0) ??
        "success"
    const page = selection?.page ?? 1
    const items = useBackgroundJobItemsQuery(
        canReadItems ? job.id : null,
        page,
        !finished,
        { status, pageSize: 10 },
    )
    const download = useProductImportResultsDownload(job.id, job.job_no)
    const id = `background-job-product-import-${toAutomationIdSegment(job.id)}`
    const progressLabel = `已处理 ${job.processed_count} / ${job.total_count} 行`
    const selectedLabel = PRODUCT_IMPORT_RESULT_LABELS[status]

    return (
        <section className="space-y-3" aria-label="商品导入结果">
            <div className="flex flex-wrap items-center justify-between gap-2">
                <h3 className="font-medium">
                    {finished ? "导入结果" : "导入进度"}
                </h3>
                {canList && (
                    <Button
                        id={`${id}-open-list`}
                        variant="link"
                        size="sm"
                        className="h-auto px-0"
                        render={<Link href="/master-data/products" />}
                    >
                        前往商品列表
                        <ArrowUpRightIcon data-icon="inline-end" aria-hidden />
                    </Button>
                )}
            </div>
            {finished ? (
                <p className="num text-muted-foreground">{progressLabel}</p>
            ) : (
                <Progress
                    value={job.processed_count}
                    max={job.total_count || 1}
                    aria-label="已处理导入行数"
                >
                    <ProgressLabel>{progressLabel}</ProgressLabel>
                </Progress>
            )}
            {job.error_summary && (
                <p role="alert" className="text-destructive">
                    {job.error_summary}
                </p>
            )}
            <div
                className="grid grid-cols-2 gap-2"
                role="group"
                aria-label="按处理结果查看"
            >
                {statuses.map((category) => {
                    const content = (
                        <>
                            <span>
                                {PRODUCT_IMPORT_RESULT_LABELS[category]}
                            </span>
                            <span
                                className={cn(
                                    "num",
                                    category === "failed" &&
                                        counts.failed > 0 &&
                                        "text-destructive",
                                )}
                            >
                                {counts[category]}
                            </span>
                        </>
                    )
                    return canReadItems ? (
                        <Button
                            key={category}
                            id={`${id}-filter-${category}`}
                            size="sm"
                            variant="outline"
                            aria-pressed={status === category}
                            className="justify-between aria-pressed:border-foreground/40 aria-pressed:bg-accent"
                            onClick={() =>
                                setSelection({ status: category, page: 1 })
                            }
                        >
                            {content}
                        </Button>
                    ) : (
                        <div
                            key={category}
                            className="flex justify-between gap-2 rounded-lg border px-3 py-2"
                        >
                            {content}
                        </div>
                    )
                })}
            </div>
            {canReadItems && (
                <BackgroundJobItemResults
                    previewJob={job}
                    itemsPage={page}
                    onPageChange={(page) => setSelection({ status, page })}
                    itemsQuery={items}
                    title={`${selectedLabel}行`}
                    feedback={
                        canDownload &&
                        download.isError &&
                        download.variables === status && (
                            <p role="alert" className="text-destructive">
                                {getErrorMessage(
                                    download.error,
                                    "下载失败，请重试",
                                )}
                            </p>
                        )
                    }
                    action={
                        canDownload &&
                        finished &&
                        counts[status] > 0 && (
                            <LoadingButton
                                id={`${id}-download-${status}`}
                                size="sm"
                                variant="outline"
                                loading={
                                    download.isPending &&
                                    download.variables === status
                                }
                                disabled={download.isPending}
                                onClick={() => download.mutate(status)}
                            >
                                <DownloadIcon
                                    data-icon="inline-start"
                                    aria-hidden
                                />
                                {download.isPending &&
                                download.variables === status
                                    ? "准备下载…"
                                    : "下载 Excel"}
                            </LoadingButton>
                        )
                    }
                />
            )}
        </section>
    )
}
