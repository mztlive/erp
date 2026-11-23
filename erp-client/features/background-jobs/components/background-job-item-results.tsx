"use client"

import type { ReactNode } from "react"
import { BusinessFailureState } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { BackgroundJobView } from "../api"
import type { useBackgroundJobItemsQuery } from "../queries"
import { usesFilteredImportResults } from "../labels"
import { BACKGROUND_JOBS_ID_PREFIX as ID_PREFIX } from "../lib/constants"
import {
    DirectImportItemIdentity,
    directImportItemResultLabel,
} from "./direct-import-result"

type BackgroundJobItemResultsProps = {
    previewJob: BackgroundJobView
    itemsPage: number
    onPageChange: (page: number) => void
    itemsQuery: ReturnType<typeof useBackgroundJobItemsQuery>
    title?: string
    action?: ReactNode
    feedback?: ReactNode
}

/** 显示当前结果页的源位置、状态和处理说明。 */
export function BackgroundJobItemResults({
    previewJob,
    itemsPage,
    onPageChange,
    itemsQuery,
    title = "逐项结果",
    action,
    feedback,
}: BackgroundJobItemResultsProps) {
    const page = itemsQuery.data
    const totalPages = page
        ? Math.max(1, Math.ceil(page.total / page.page_size))
        : 1
    const filteredImport = usesFilteredImportResults(previewJob.domain_job_type)
    return (
        <section className="space-y-3">
            <div className="flex flex-wrap items-center justify-between gap-2">
                <h3 className="font-medium">{title}</h3>
                {action}
            </div>
            {feedback}
            {page && (totalPages > 1 || itemsPage > 1) && (
                <div className="mt-3 flex items-center justify-end gap-3">
                    <Button
                        id={`${ID_PREFIX}-items-prev`}
                        variant="outline"
                        disabled={itemsPage === 1}
                        onClick={() => onPageChange(itemsPage - 1)}
                    >
                        上一页
                    </Button>
                    <span className="text-body-compact">
                        第 {itemsPage} 页，共 {totalPages} 页
                    </span>
                    <Button
                        id={`${ID_PREFIX}-items-next`}
                        variant="outline"
                        disabled={itemsPage >= totalPages}
                        onClick={() => onPageChange(itemsPage + 1)}
                    >
                        下一页
                    </Button>
                </div>
            )}
            {itemsQuery.isPending ||
            (filteredImport && itemsQuery.isPlaceholderData) ? (
                <p className="text-muted-foreground">正在加载逐项结果…</p>
            ) : itemsQuery.isError ? null : (itemsQuery.data?.items.length ??
                  0) === 0 ? (
                <p className="text-muted-foreground">暂无逐项结果。</p>
            ) : (
                <ul className="space-y-2">
                    {page?.items.map((item) => {
                        const summary =
                            item.result_summary ||
                            (filteredImport
                                ? item.status === "failed"
                                    ? "未记录失败原因"
                                    : null
                                : previewJob.finished_at
                                  ? "未处理"
                                  : "等待处理")
                        return (
                            <li key={item.id} className="rounded-lg border p-3">
                                <div className="flex items-center justify-between gap-2">
                                    <span className="num text-xs text-muted-foreground">
                                        {item.source_row_no
                                            ? `Excel 第 ${item.source_row_no} 行`
                                            : `第 ${item.item_no} 项`}
                                    </span>
                                    <span
                                        className={
                                            item.status === "failed" &&
                                            filteredImport
                                                ? "text-xs text-destructive"
                                                : "text-xs"
                                        }
                                    >
                                        {filteredImport ||
                                        previewJob.domain_job_type ===
                                            "SUPPLIER_IMPORT" ||
                                        previewJob.domain_job_type ===
                                            "BRAND_IMPORT"
                                            ? directImportItemResultLabel(item)
                                            : item.result_code ===
                                                "outcome_unknown"
                                              ? "结果待核对"
                                              : ({
                                                    success: "执行完成",
                                                    skipped: "已跳过",
                                                    failed: "执行失败",
                                                }[item.status ?? ""] ??
                                                "未处理")}
                                    </span>
                                </div>
                                <DirectImportItemIdentity item={item} />
                                {summary && (
                                    <p className="mt-1 text-body-compact">
                                        {summary}
                                    </p>
                                )}
                            </li>
                        )
                    })}
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
