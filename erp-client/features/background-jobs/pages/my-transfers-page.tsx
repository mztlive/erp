"use client"
import { useState } from "react"
import { useMutation, useQuery } from "@tanstack/react-query"
import { apiGet } from "@/lib/api"
import { Button } from "@/components/ui/button"
import { getErrorMessage } from "@/lib/api/errors"
import {
    readTransferResult,
    downloadTransfer,
    type TransferJob,
} from "@/lib/data-transfer"
import { toAutomationIdSegment } from "@/lib/automation-id"
const labels: Record<string, string> = {
    pending: "排队中",
    running: "处理中",
    succeeded: "处理完成",
    partially_succeeded: "部分完成",
    failed: "处理失败",
    cancelled: "已取消",
}
export function MyTransfersPage({ portal = false }: { portal?: boolean }) {
    const [page, setPage] = useState(1)
    const query = useQuery({
        queryKey: [
            portal ? "supplier-portal" : "account",
            "data-transfers",
            page,
        ],
        queryFn: () =>
            apiGet<{ items: TransferJob[]; total: number }>(
                portal
                    ? "/supplier-portal/data-transfers"
                    : "/account/data-transfers",
                {
                    page,
                    page_size: 20,
                    sort_by: "created_at",
                    sort_dir: "desc",
                },
                portal ? { session: "supplier-portal" } : undefined,
            ),
        refetchInterval: 5000,
    })
    const download = useMutation({
        mutationFn: async (id: string) =>
            downloadTransfer(await readTransferResult(id, portal)),
    })
    return (
        <main className="mx-auto max-w-5xl space-y-4 p-6">
            <h1 className="text-xl font-semibold">我的文件任务</h1>
            <p className="text-sm text-muted-foreground">
                关闭页面后任务继续处理。文件结果保留 7
                天；导入预览仍受业务确认有效期限制。下载时会重新检查当前权限。
            </p>
            {query.isPending && <p>正在加载…</p>}
            {query.isError && (
                <p className="text-destructive">
                    {getErrorMessage(query.error, "任务读取失败，请重试")}
                </p>
            )}
            {download.isError && (
                <p className="text-destructive">
                    {getErrorMessage(download.error, "下载失败，请重试")}
                </p>
            )}
            <Button
                id="my-transfers-refresh"
                variant="outline"
                onClick={() => void query.refetch()}
            >
                刷新
            </Button>
            {query.data?.items.length === 0 && <p>暂无文件任务。</p>}
            <ul className="space-y-3">
                {query.data?.items.map((job) => (
                    <li
                        key={job.id}
                        className="flex flex-wrap items-center justify-between gap-3 rounded-lg border p-4"
                    >
                        <div>
                            <p>
                                {job.job_no} ·{" "}
                                {labels[job.status.toLowerCase()] ?? "处理中"}
                            </p>
                            <p className="text-sm text-muted-foreground">
                                已处理 {job.processed_count} / {job.total_count}
                            </p>
                            {job.error_summary && (
                                <p className="text-sm text-destructive">
                                    {job.error_summary}
                                </p>
                            )}
                        </div>
                        {job.finished_at && job.result_file_asset_id && (
                            <Button
                                id={`my-transfers-download-${toAutomationIdSegment(job.id)}`}
                                disabled={download.isPending}
                                onClick={() => download.mutate(job.id)}
                            >
                                下载结果
                            </Button>
                        )}
                    </li>
                ))}
            </ul>
            <div className="flex gap-3">
                <Button
                    id="my-transfers-prev"
                    disabled={page <= 1}
                    onClick={() => setPage((p) => p - 1)}
                >
                    上一页
                </Button>
                <span>第 {page} 页</span>
                <Button
                    id="my-transfers-next"
                    disabled={page * 20 >= (query.data?.total ?? 0)}
                    onClick={() => setPage((p) => p + 1)}
                >
                    下一页
                </Button>
            </div>
        </main>
    )
}
