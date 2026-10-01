"use client"

import type { Dispatch, SetStateAction } from "react"
import { BusinessFailureState } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { BackgroundJobView } from "../api"
import type { useBackgroundJobItemsQuery } from "../queries"
import { BACKGROUND_JOBS_ID_PREFIX as ID_PREFIX } from "../lib/constants"
import { SupplierImportFailures } from "./supplier-import-failures"

type BackgroundJobItemResultsProps = {
    previewJob: BackgroundJobView
    currentUserId: string | undefined
    itemsPage: number
    onPageChange: Dispatch<SetStateAction<number>>
    itemsQuery: ReturnType<typeof useBackgroundJobItemsQuery>
}

/** 逐项结果、结果分页与原提交人的供应商待处理行下载。 */
export function BackgroundJobItemResults({
    previewJob,
    currentUserId,
    itemsPage,
    onPageChange,
    itemsQuery,
}: BackgroundJobItemResultsProps) {
    return (
        <section className="space-y-3">
            <h3 className="font-medium">逐项结果</h3>
            {(itemsQuery.data?.total ?? 0) > 100 && (
                <div className="mt-3 flex items-center justify-end gap-3">
                    <Button
                        id={`${ID_PREFIX}-items-prev`}
                        variant="outline"
                        disabled={itemsPage === 1}
                        onClick={() => onPageChange((page) => page - 1)}
                    >
                        上一页
                    </Button>
                    <span className="text-sm">
                        第 {itemsPage} 页，共{" "}
                        {Math.ceil((itemsQuery.data?.total ?? 0) / 100)} 页
                    </span>
                    <Button
                        id={`${ID_PREFIX}-items-next`}
                        variant="outline"
                        disabled={
                            itemsPage * 100 >= (itemsQuery.data?.total ?? 0)
                        }
                        onClick={() => onPageChange((page) => page + 1)}
                    >
                        下一页
                    </Button>
                </div>
            )}
            {previewJob.domain_job_type === "SUPPLIER_IMPORT" &&
                previewJob.finished_at !== null &&
                previewJob.requested_by === currentUserId &&
                (previewJob.failed_count > 0 ||
                    previewJob.processed_count < previewJob.total_count) && (
                    <div className="mt-4">
                        <SupplierImportFailures
                            key={previewJob.id}
                            jobId={previewJob.id}
                        />
                    </div>
                )}
            {itemsQuery.isPending ? (
                <p className="text-muted-foreground">正在加载逐项结果…</p>
            ) : (itemsQuery.data?.items.length ?? 0) === 0 ? (
                <p className="text-muted-foreground">暂无逐项结果。</p>
            ) : (
                <ul className="space-y-2">
                    {itemsQuery.data?.items.map((item) => (
                        <li key={item.id} className="rounded-lg border p-3">
                            <div className="flex items-center justify-between gap-2">
                                <span className="num text-xs text-muted-foreground">
                                    {item.source_row_no
                                        ? `Excel 第 ${item.source_row_no} 行`
                                        : `第 ${item.item_no} 项`}
                                </span>
                                <span className="text-xs">
                                    {item.result_code === "outcome_unknown"
                                        ? "结果待确认"
                                        : ({
                                              success: "成功",
                                              skipped: "跳过",
                                              failed: "执行失败",
                                          }[item.status ?? ""] ?? "待执行")}
                                </span>
                            </div>
                            {item.object_type === "supplier_import_row" && (
                                <p className="mt-1 font-medium">
                                    {item.object_id}
                                </p>
                            )}
                            <p className="mt-1 text-[13px]">
                                {item.result_summary || "排队执行中"}
                            </p>
                        </li>
                    ))}
                </ul>
            )}
            {itemsQuery.isError && (
                <BusinessFailureState
                    error={itemsQuery.error}
                    action={
                        <Button
                            id={`${ID_PREFIX}-items-retry`}
                            variant="outline"
                            onClick={() => void itemsQuery.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            )}
        </section>
    )
}
