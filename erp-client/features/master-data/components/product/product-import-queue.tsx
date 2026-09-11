"use client"

import { useState } from "react"
import { BackgroundJobProgress } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    useProductImportItemsQuery,
    useProductImportJobsQuery,
} from "@/features/master-data/hooks/use-product-import"
import {
    isProductImportActive,
    PRODUCT_IMPORT_ITEM_LABELS,
    PRODUCT_IMPORT_STATUS_LABELS,
    productImportProgressStatus,
    visibleProductImportJobs,
} from "@/features/master-data/lib/product-import"

export function ProductImportQueue({
    enabled,
    sessionJobIds,
    onDismiss,
}: {
    enabled: boolean
    sessionJobIds: readonly string[]
    onDismiss: (jobId: string) => void
}) {
    const jobsQuery = useProductImportJobsQuery(enabled)
    const [openJobId, setOpenJobId] = useState<string | null>(null)
    const itemsQuery = useProductImportItemsQuery(openJobId)
    const jobs = visibleProductImportJobs(
        jobsQuery.data?.items ?? [],
        sessionJobIds,
    )
    if (!enabled || jobs.length === 0) return null

    return (
        <div className="space-y-3" id="master-data-products-import-queue">
            {jobs.map((job) => (
                <BackgroundJobProgress
                    key={job.id}
                    mode="partialAllowed"
                    status={productImportProgressStatus(job.status)}
                    total={job.total_count}
                    completed={job.processed_count}
                    succeeded={job.success_count}
                    skipped={job.skipped_count}
                    failed={job.failed_count}
                    label={job.file_name || job.job_no}
                    description={
                        <>
                            {PRODUCT_IMPORT_STATUS_LABELS[
                                job.status as keyof typeof PRODUCT_IMPORT_STATUS_LABELS
                            ] ?? "导入任务"}
                            。任务号{" "}
                            <span className="num">{job.job_no}</span>
                            {job.error_summary ? `。${job.error_summary}` : ""}
                        </>
                    }
                    action={
                        <div className="flex flex-wrap gap-2">
                            <Button
                                id={`master-data-products-import-job-${job.id}-items`}
                                type="button"
                                size="sm"
                                variant="outline"
                                onClick={() =>
                                    setOpenJobId((current) =>
                                        current === job.id ? null : job.id,
                                    )
                                }
                            >
                                {openJobId === job.id
                                    ? "收起结果"
                                    : "查看结果"}
                            </Button>
                            {isProductImportActive(job.status) ? null : (
                                <Button
                                    id={`master-data-products-import-job-${job.id}-dismiss`}
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    onClick={() => {
                                        if (openJobId === job.id)
                                            setOpenJobId(null)
                                        onDismiss(job.id)
                                    }}
                                >
                                    关闭
                                </Button>
                            )}
                        </div>
                    }
                />
            ))}
            {openJobId && itemsQuery.data ? (
                <div className="max-h-80 overflow-auto rounded-lg border">
                    <table className="w-full text-left text-sm">
                        <thead className="sticky top-0 bg-muted">
                            <tr>
                                <th className="p-3">Excel 行</th>
                                <th className="p-3">商品名称</th>
                                <th className="p-3">结果</th>
                                <th className="p-3">说明</th>
                            </tr>
                        </thead>
                        <tbody>
                            {itemsQuery.data.items.map((item) => (
                                <tr
                                    key={item.item_no}
                                    className="border-t"
                                >
                                    <td className="p-3">
                                        {item.source_row_no ?? item.item_no}
                                    </td>
                                    <td className="min-w-48 p-3">
                                        {item.name || "未填写"}
                                    </td>
                                    <td className="whitespace-nowrap p-3">
                                        {item.status
                                            ? (PRODUCT_IMPORT_ITEM_LABELS[
                                                  item.status as keyof typeof PRODUCT_IMPORT_ITEM_LABELS
                                              ] ?? item.status)
                                            : "待导入"}
                                    </td>
                                    <td className="min-w-56 p-3">
                                        {item.result_summary || "排队导入中"}
                                    </td>
                                </tr>
                            ))}
                        </tbody>
                    </table>
                </div>
            ) : null}
        </div>
    )
}
