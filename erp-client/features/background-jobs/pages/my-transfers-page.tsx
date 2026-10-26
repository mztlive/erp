"use client"
import { useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { apiGet } from "@/lib/api"
import { Button } from "@/components/ui/button"
import { getErrorMessage } from "@/lib/api/errors"
import type { BackgroundJobView } from "../api"
import { MyTransferPreview } from "../components/my-transfer-preview"
import { JOB_STATUS_LABELS, backgroundJobIdentity } from "../labels"
import { toAutomationIdSegment } from "@/lib/automation-id"
export function MyTransfersPage({ portal = false }: { portal?: boolean }) {
    const [page, setPage] = useState(1)
    const [previewId, setPreviewId] = useState<string | null>(null)
    const query = useQuery({
        queryKey: [
            portal ? "supplier-portal" : "account",
            "data-transfers",
            page,
        ],
        queryFn: () =>
            apiGet<{ items: BackgroundJobView[]; total: number }>(
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
            <Button
                id="my-transfers-refresh"
                variant="outline"
                onClick={() => void query.refetch()}
            >
                刷新
            </Button>
            {!query.isError && query.data?.items.length === 0 && (
                <p>暂无文件任务。</p>
            )}
            <ul className="space-y-3">
                {query.data?.items.map((job) => (
                    <li
                        key={job.id}
                        className="flex flex-wrap items-center justify-between gap-3 rounded-lg border p-4"
                    >
                        <div>
                            <p>
                                {backgroundJobIdentity(job).title} ·{" "}
                                {JOB_STATUS_LABELS[job.status]}
                            </p>
                            <p className="text-sm text-muted-foreground">
                                {backgroundJobIdentity(job).number}
                            </p>
                            {job.error_summary && (
                                <p className="text-sm text-destructive">
                                    {job.error_summary}
                                </p>
                            )}
                        </div>
                        <Button
                            id={`my-transfers-open-${toAutomationIdSegment(job.id)}`}
                            variant="outline"
                            onClick={() => setPreviewId(job.id)}
                        >
                            查看结果与后续操作
                        </Button>
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
            <MyTransferPreview
                jobId={previewId}
                portal={portal}
                onClose={() => setPreviewId(null)}
            />
        </main>
    )
}
